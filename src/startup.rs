//! Startup and Indexing Utilities
//!
//! This module contains functions for workspace initialization, staleness detection,
//! and automatic indexing on server startup.

use crate::handler::JulieServerHandler;
use crate::workspace::startup_hint::WorkspaceStartupSource;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, info, warn};

pub(crate) fn startup_source_prefers_request_roots(source: Option<WorkspaceStartupSource>) -> bool {
    matches!(source, Some(WorkspaceStartupSource::Cwd))
}

/// Checkpoint the active workspace database WAL if a workspace is initialized.
pub async fn checkpoint_active_workspace_wal(
    handler: &JulieServerHandler,
) -> Result<Option<(i32, i32, i32)>> {
    let primary_snapshot = match handler.primary_workspace_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            if handler.is_primary_workspace_swap_in_progress() {
                return Err(err);
            }

            if handler.get_workspace().await?.is_none() {
                return Ok(None);
            }

            return Err(err);
        }
    };
    let db_arc = primary_snapshot.database;

    tokio::task::spawn_blocking(move || -> Result<Option<(i32, i32, i32)>> {
        let mut db = db_arc.try_lock().map_err(|e| {
            anyhow::anyhow!("Could not acquire database lock for checkpoint: {}", e)
        })?;
        Ok(Some(db.checkpoint_wal()?))
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to join checkpoint task: {}", e))?
}

/// Check if the workspace needs indexing by examining database state
///
/// This function checks:
/// 1. If the database is completely empty (requires full index)
/// 2. If files have been modified since last index (staleness)
/// 3. If new files exist that aren't in the database
pub async fn check_if_indexing_needed(handler: &JulieServerHandler) -> Result<bool> {
    let primary_snapshot = match handler.primary_workspace_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            if handler.is_primary_workspace_swap_in_progress() {
                return Err(err);
            }

            if handler.get_workspace().await?.is_none() {
                debug!("No workspace found - indexing needed");
                return Ok(true);
            }

            return Err(err);
        }
    };

    let current_primary_root = primary_snapshot.binding.workspace_root.clone();
    let db_path = crate::handler::metrics_db_path_for_workspace(
        primary_snapshot.index_root_override.as_deref(),
        &current_primary_root,
        &primary_snapshot.binding.workspace_id,
    );
    let db_arc = Arc::clone(&primary_snapshot.database);

    if !db_path.exists() {
        debug!("No database connection - indexing needed");
        return Ok(true);
    }

    // Now lock database (no await while holding this lock)
    let db: std::sync::MutexGuard<'_, crate::database::SymbolDatabase> = match db_arc.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!(
                "Database mutex poisoned during startup check, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };

    match db.has_symbols_for_workspace() {
        Ok(has_symbols) => {
            if !has_symbols {
                info!("📊 Database is empty - indexing needed");
                return Ok(true);
            }

            // ✅ NEW: Check if index is stale
            // Compare file modification times with database timestamp
            let db_mtime = get_database_mtime(&db_path)?;
            let max_file_mtime = get_max_file_mtime_in_workspace(&current_primary_root)?;

            debug!(
                "Staleness check: db_mtime={:?}, max_file_mtime={:?}, stale={}",
                db_mtime,
                max_file_mtime,
                max_file_mtime > db_mtime
            );

            if max_file_mtime > db_mtime {
                info!("📊 Database is stale (files modified after last index) - indexing needed");
                return Ok(true);
            }

            // ✅ NEW: Check for new files not in database
            let indexed_files_raw: Vec<String> = db.get_all_indexed_files()?;

            // Database stores relative Unix-style paths per CLAUDE.md Path Handling Contract
            // No normalization needed - indexed_files are already relative
            let indexed_files: HashSet<String> = indexed_files_raw.into_iter().collect();

            let workspace_files = scan_workspace_files(&current_primary_root)?;
            let new_files: Vec<_> = workspace_files.difference(&indexed_files).collect();

            debug!(
                "New file check: indexed={}, workspace={}, new={}",
                indexed_files.len(),
                workspace_files.len(),
                new_files.len()
            );

            if !new_files.is_empty() {
                info!(
                    "📊 Found {} new files not in database - indexing needed",
                    new_files.len()
                );
                debug!("New files: {:?}", new_files);
                return Ok(true);
            }

            // Check for deleted files (indexed but no longer on disk)
            let deleted_files: Vec<_> = indexed_files.difference(&workspace_files).collect();

            if !deleted_files.is_empty() {
                info!(
                    "📊 Found {} deleted files still in database - cleanup needed",
                    deleted_files.len()
                );
                debug!("Deleted files: {:?}", deleted_files);
                // Note: returning true triggers index_workspace_files, which calls
                // filter_changed_files -> clean_orphaned_files. This cleans up the
                // deleted files' symbols and DB records.
                return Ok(true);
            }

            info!("✅ Index is up-to-date - no indexing needed");
            Ok(false)
        }
        Err(e) => {
            debug!(
                "Error checking database symbols: {} - assuming indexing needed",
                e
            );
            Ok(true)
        }
    }
}

/// Get the modification time of the SQLite database file
///
/// Returns the mtime of the symbols.db file for the given workspace
fn get_database_mtime(db_path: &Path) -> Result<SystemTime> {
    if !db_path.exists() {
        // Database doesn't exist - return epoch (very old time)
        return Ok(SystemTime::UNIX_EPOCH);
    }

    let metadata = std::fs::metadata(&db_path)
        .with_context(|| format!("Failed to get metadata for database: {}", db_path.display()))?;

    metadata
        .modified()
        .with_context(|| format!("Failed to get mtime for database: {}", db_path.display()))
}

/// Get the maximum (newest) file modification time in the workspace
///
/// Scans all supported code files and returns the newest mtime found
fn get_max_file_mtime_in_workspace(workspace_root: &Path) -> Result<SystemTime> {
    use crate::utils::walk::{WalkConfig, build_walker};

    let mut max_mtime = SystemTime::UNIX_EPOCH;

    for result in build_walker(workspace_root, &WalkConfig::stale_scan()) {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        if !is_code_file(entry.path()) {
            continue;
        }

        if let Ok(metadata) = std::fs::metadata(entry.path()) {
            if let Ok(mtime) = metadata.modified() {
                if mtime > max_mtime {
                    max_mtime = mtime;
                }
            }
        }
    }

    Ok(max_mtime)
}

/// Scan workspace and return a set of all code file paths (relative to workspace root)
///
/// This is used to detect new files that aren't in the database yet
pub(crate) fn scan_workspace_files(workspace_root: &Path) -> Result<HashSet<String>> {
    use crate::utils::walk::{WalkConfig, build_walker};

    let mut files = HashSet::new();

    for result in build_walker(workspace_root, &WalkConfig::stale_scan()) {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        if !is_code_file(entry.path()) {
            continue;
        }

        // Get relative path from workspace root in Unix-style format
        // CRITICAL: Use to_relative_unix_style() to ensure cross-platform compatibility
        // On Windows, strip_prefix() returns paths with backslashes (src\file.rs)
        // But database stores paths with forward slashes (src/file.rs)
        if let Ok(relative_path) =
            crate::utils::paths::to_relative_unix_style(entry.path(), workspace_root)
        {
            files.insert(relative_path);
        }
    }

    Ok(files)
}

/// Check if a file is a supported code file.
///
/// Accepts files by extension (delegates to `build_supported_extensions()` so
/// this function and the file watcher share a single canonical list from
/// julie_extractors), plus extensionless files that pass the shared
/// `is_likely_text_file` heuristic. Files whose name is in
/// `BLACKLISTED_FILENAMES` are rejected in either branch. The goal is to stay
/// in sync with `ManageWorkspaceTool::should_index_file()` — otherwise the
/// freshness scan and the indexer disagree on what belongs in the tracked
/// set, causing phantom "new/deleted file" signals on every reconnect. See
/// Finding #3 in `docs/ROOTS_IMPL_REVIEW_NOTES.md`.
fn is_code_file(path: &Path) -> bool {
    use std::sync::OnceLock;
    static SUPPORTED: OnceLock<HashSet<String>> = OnceLock::new();
    let supported = SUPPORTED.get_or_init(crate::watcher::filtering::build_supported_extensions);

    // Match the indexer's filename blacklist so extensionless files like
    // `.julieignore` stay out of both sides of the freshness diff.
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if crate::tools::shared::BLACKLISTED_FILENAMES.contains(&file_name) {
            return false;
        }
    }

    match path.extension() {
        Some(ext) => supported.contains(ext.to_string_lossy().to_lowercase().as_str()),
        None => crate::utils::file_utils::is_likely_text_file(path),
    }
}
