//! User-level project registry for cross-project features.
//!
//! Maintains a global registry at `~/.julie/project_registry.json` that tracks
//! all Julie-enabled projects. Projects auto-register on MCP initialization,
//! enabling cross-project features like workspace discovery.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::workspace::registry::generate_workspace_id;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// User-level project registry (stored at ~/.julie/project_registry.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProjectRegistry {
    pub version: u32,
    pub projects: HashMap<String, ProjectEntry>,
}

impl Default for UserProjectRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            projects: HashMap::new(),
        }
    }
}

/// Entry for a registered project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub project_id: String,
    pub name: String,
    pub path: String,
    pub registered_at: i64,
    pub last_opened: i64,
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Returns the user-level Julie directory (`~/.julie/`), creating it if needed.
pub fn get_user_julie_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Cannot determine home directory (HOME / USERPROFILE not set)")?;

    let dir = PathBuf::from(home).join(".julie");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create {}", dir.display()))?;
    }
    Ok(dir)
}

/// Returns the default registry path: `~/.julie/project_registry.json`.
pub fn get_registry_path() -> Result<PathBuf> {
    Ok(get_user_julie_dir()?.join("project_registry.json"))
}

// ---------------------------------------------------------------------------
// Core operations
// ---------------------------------------------------------------------------

/// Register a project in the user-level registry (idempotent).
///
/// - First call creates the entry with `registered_at` and `last_opened`.
/// - Subsequent calls only update `last_opened`.
///
/// Uses the default registry path (`~/.julie/project_registry.json`).
pub fn register_project(workspace_root: &Path) -> Result<()> {
    let registry_path = get_registry_path()?;
    register_project_at(workspace_root, &registry_path)
}

/// Register a project using an explicit registry path (for testing).
pub fn register_project_at(workspace_root: &Path, registry_path: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = registry_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let path_str = workspace_root.to_string_lossy().to_string();
    let project_id = generate_workspace_id(&path_str)?;

    let name = workspace_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace")
        .to_string();

    let now = chrono::Utc::now().timestamp();

    // Read existing registry (or default)
    let mut registry = read_registry(registry_path)?;

    registry
        .projects
        .entry(project_id.clone())
        .and_modify(|entry| {
            entry.last_opened = now;
            // Update path in case it changed (e.g. moved directory)
            entry.path = path_str.clone();
            debug!("Updated project in registry: {} ({})", project_id, path_str);
        })
        .or_insert_with(|| {
            info!(
                "Registered new project: {} ({}) as {}",
                name, path_str, project_id
            );
            ProjectEntry {
                project_id,
                name,
                path: path_str,
                registered_at: now,
                last_opened: now,
            }
        });

    write_registry_atomic(registry_path, &registry)
}

/// List all registered projects, sorted by `last_opened` (most recent first).
///
/// Uses the default registry path (`~/.julie/project_registry.json`).
pub fn list_projects() -> Result<Vec<ProjectEntry>> {
    let registry_path = get_registry_path()?;
    list_projects_at(&registry_path)
}

/// List all registered projects from an explicit registry path (for testing).
pub fn list_projects_at(registry_path: &Path) -> Result<Vec<ProjectEntry>> {
    if !registry_path.exists() {
        return Ok(Vec::new());
    }

    let registry = read_registry(registry_path)?;
    let mut projects: Vec<ProjectEntry> = registry.projects.into_values().collect();
    projects.sort_by(|a, b| b.last_opened.cmp(&a.last_opened));
    Ok(projects)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Read and parse the registry file. Returns default if missing or corrupt.
fn read_registry(path: &Path) -> Result<UserProjectRegistry> {
    if !path.exists() {
        return Ok(UserProjectRegistry::default());
    }

    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read registry at {}", path.display()))?;

    // Gracefully handle corrupt JSON — start fresh rather than error
    match serde_json::from_str::<UserProjectRegistry>(&contents) {
        Ok(registry) => Ok(registry),
        Err(e) => {
            tracing::warn!(
                "Registry file corrupt ({}), starting fresh: {}",
                path.display(),
                e
            );
            Ok(UserProjectRegistry::default())
        }
    }
}

/// Write registry atomically: write to temp file, then rename.
fn write_registry_atomic(path: &Path, registry: &UserProjectRegistry) -> Result<()> {
    let contents = serde_json::to_string_pretty(registry)
        .context("Failed to serialize registry")?;

    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, &contents)
        .with_context(|| format!("Failed to write temp registry at {}", temp_path.display()))?;

    std::fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Failed to rename {} → {}",
            temp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}
