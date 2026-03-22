//! Safe index migration from per-project (.julie/indexes/) to centralized (~/.julie/indexes/).
//!
//! When the daemon starts, existing per-project indexes need to be migrated to the
//! centralized location. This module handles discovery, copy-validate-delete migration,
//! and state tracking to avoid re-migrating.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::paths::DaemonPaths;

/// Tracks which workspace indexes have been successfully migrated.
/// Persisted as JSON so migration survives daemon restarts.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MigrationState {
    /// Workspace IDs that have been successfully migrated
    migrated: HashSet<String>,
    #[serde(skip)]
    path: PathBuf,
}

impl MigrationState {
    /// Create a new empty migration state that will save to `path`.
    pub fn new(path: &Path) -> Self {
        Self {
            migrated: HashSet::new(),
            path: path.to_path_buf(),
        }
    }

    /// Load migration state from a JSON file, or return empty state if file doesn't exist.
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let data = fs::read_to_string(path).with_context(|| {
                format!("Failed to read migration state from {}", path.display())
            })?;
            let mut state: MigrationState = serde_json::from_str(&data).with_context(|| {
                format!("Failed to parse migration state from {}", path.display())
            })?;
            state.path = path.to_path_buf();
            Ok(state)
        } else {
            Ok(Self::new(path))
        }
    }

    /// Check if a workspace has already been migrated.
    pub fn is_migrated(&self, workspace_id: &str) -> bool {
        self.migrated.contains(workspace_id)
    }

    /// Mark a workspace as successfully migrated.
    pub fn mark_migrated(&mut self, workspace_id: &str) {
        self.migrated.insert(workspace_id.to_string());
    }

    /// Persist migration state to disk as JSON.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create directory for migration state: {}",
                    parent.display()
                )
            })?;
        }
        let data =
            serde_json::to_string_pretty(&self).context("Failed to serialize migration state")?;
        fs::write(&self.path, data).with_context(|| {
            format!("Failed to write migration state to {}", self.path.display())
        })?;
        Ok(())
    }
}

/// Recursively copy a directory tree from `src` to `dst`.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create directory {}", dst.display()))?;

    for entry in
        fs::read_dir(src).with_context(|| format!("Failed to read directory {}", src.display()))?
    {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "Failed to copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Migrate a single workspace index from per-project to centralized location.
/// Uses copy-validate-delete for safety.
///
/// Steps:
/// 1. If destination already exists, skip (already migrated or created by daemon)
/// 2. Copy source directory recursively to destination
/// 3. Validate: check db/symbols.db and tantivy/meta.json exist in destination
/// 4. On successful validation: delete source directory
/// 5. On validation failure: delete incomplete destination, return error
pub fn migrate_workspace_index(
    workspace_id: &str,
    source: &Path,
    destination: &Path,
) -> Result<()> {
    // Step 1: skip if destination already exists
    if destination.exists() {
        tracing::info!(
            workspace_id,
            "Skipping migration: destination already exists at {}",
            destination.display()
        );
        return Ok(());
    }

    // Step 2: copy source to destination
    tracing::info!(
        workspace_id,
        "Migrating index from {} to {}",
        source.display(),
        destination.display()
    );
    copy_dir_recursive(source, destination)?;

    // Step 3: validate the copy
    let db_path = destination.join("db").join("symbols.db");
    let meta_path = destination.join("tantivy").join("meta.json");

    if db_path.exists() && meta_path.exists() {
        // Step 4: validation passed, delete source
        fs::remove_dir_all(source)
            .with_context(|| format!("Failed to delete migrated source at {}", source.display()))?;
        tracing::info!(workspace_id, "Migration complete, source deleted");
        Ok(())
    } else {
        // Step 5: validation failed, clean up incomplete destination
        let _ = fs::remove_dir_all(destination);
        anyhow::bail!(
            "Migration validation failed for workspace '{}': missing {} {}",
            workspace_id,
            if !db_path.exists() {
                "db/symbols.db"
            } else {
                ""
            },
            if !meta_path.exists() {
                "tantivy/meta.json"
            } else {
                ""
            },
        );
    }
}

/// Check if a directory name looks like a workspace ID (name_hash8 format).
/// The hash portion is 8 hex characters after the last underscore.
fn is_workspace_id(name: &str) -> bool {
    if let Some(pos) = name.rfind('_') {
        let hash_part = &name[pos + 1..];
        hash_part.len() == 8 && hash_part.chars().all(|c| c.is_ascii_hexdigit())
    } else {
        false
    }
}

/// Scan a project directory for existing per-project indexes.
/// Returns Vec<(workspace_id, source_path)> for indexes that need migration.
pub fn scan_project_indexes(project_root: &Path) -> Vec<(String, PathBuf)> {
    let indexes_dir = project_root.join(".julie").join("indexes");
    if !indexes_dir.exists() {
        return Vec::new();
    }

    let mut results = Vec::new();
    let entries = match fs::read_dir(&indexes_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if is_workspace_id(name) {
                    results.push((name.to_string(), path));
                }
            }
        }
    }

    results
}

/// Run migration for a workspace that's being initialized.
/// Called from the daemon during workspace setup.
///
/// 1. Load or create migration state from daemon_paths.migration_state()
/// 2. Scan for indexes in {workspace_root}/.julie/indexes/
/// 3. For each found index: if not already migrated, migrate it
/// 4. Update and save migration state
pub fn run_migration_for_workspace(
    daemon_paths: &DaemonPaths,
    workspace_root: &Path,
) -> Result<()> {
    let state_path = daemon_paths.migration_state();
    let mut state = MigrationState::load(&state_path)?;

    let indexes = scan_project_indexes(workspace_root);
    if indexes.is_empty() {
        tracing::debug!(
            "No per-project indexes found in {}",
            workspace_root.display()
        );
        return Ok(());
    }

    tracing::info!(
        "Found {} per-project index(es) in {}, checking migration status",
        indexes.len(),
        workspace_root.display()
    );

    for (workspace_id, source_path) in &indexes {
        if state.is_migrated(workspace_id) {
            tracing::debug!(workspace_id, "Already migrated, skipping");
            continue;
        }

        let destination = daemon_paths.workspace_index_dir(workspace_id);

        match migrate_workspace_index(workspace_id, source_path, &destination) {
            Ok(()) => {
                state.mark_migrated(workspace_id);
                tracing::info!(workspace_id, "Successfully migrated workspace index");
            }
            Err(e) => {
                tracing::warn!(workspace_id, "Failed to migrate workspace index: {:#}", e);
                // Continue with other indexes; don't fail the whole migration
            }
        }
    }

    state
        .save()
        .with_context(|| format!("Failed to save migration state to {}", state_path.display()))?;

    Ok(())
}
