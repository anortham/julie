//! Startup and Indexing Utilities
//!
//! This module contains functions for workspace initialization, staleness detection,
//! and automatic indexing on server startup.

use crate::handler::JulieServerHandler;
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;
use tracing::{debug, info, warn};

/// Checkpoint the active workspace database WAL if a workspace is initialized.
pub async fn checkpoint_active_workspace_wal(
    handler: &JulieServerHandler,
) -> Result<Option<(i32, i32, i32)>> {
    let workspace = match handler.get_workspace().await? {
        Some(workspace) => workspace,
        None => return Ok(None),
    };

    let Some(db_arc) = workspace.db else {
        return Ok(None);
    };

    tokio::task::spawn_blocking(move || -> Result<Option<(i32, i32, i32)>> {
        let mut db = db_arc.lock().map_err(|e| {
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
    // Get workspace to check database
    let workspace = match handler.get_workspace().await? {
        Some(ws) => ws,
        None => {
            debug!("No workspace found - indexing needed");
            return Ok(true);
        }
    };

    // Check if database exists and has symbols
    if let Some(db_arc) = &workspace.db {
        // Compute workspace ID: use daemon workspace_id if available, else derive from path
        let primary_workspace_id = if let Some(ref ws_id) = handler.workspace_id {
            ws_id.clone()
        } else {
            crate::workspace::registry::generate_workspace_id(&workspace.root.to_string_lossy())
                .unwrap_or_else(|_| {
                    debug!("Failed to generate workspace ID - indexing needed");
                    String::new()
                })
        };

        // Now lock database (no await while holding this lock)
        let db = match db_arc.lock() {
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
                let db_mtime = get_database_mtime(&workspace.root, &primary_workspace_id)?;
                let max_file_mtime = get_max_file_mtime_in_workspace(&workspace.root)?;

                debug!(
                    "Staleness check: db_mtime={:?}, max_file_mtime={:?}, stale={}",
                    db_mtime,
                    max_file_mtime,
                    max_file_mtime > db_mtime
                );

                if max_file_mtime > db_mtime {
                    info!(
                        "📊 Database is stale (files modified after last index) - indexing needed"
                    );
                    return Ok(true);
                }

                // ✅ NEW: Check for new files not in database
                let indexed_files_raw: Vec<String> = db.get_all_indexed_files()?;

                // Database stores relative Unix-style paths per CLAUDE.md Path Handling Contract
                // No normalization needed - indexed_files are already relative
                let indexed_files: HashSet<String> = indexed_files_raw.into_iter().collect();

                let workspace_files = scan_workspace_files(&workspace.root)?;
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
    } else {
        debug!("No database connection - indexing needed");
        Ok(true)
    }
}

/// Get the modification time of the SQLite database file
///
/// Returns the mtime of the symbols.db file for the given workspace
fn get_database_mtime(workspace_root: &Path, workspace_id: &str) -> Result<SystemTime> {
    let db_path = workspace_root
        .join(".julie")
        .join("indexes")
        .join(workspace_id)
        .join("db")
        .join("symbols.db");

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

/// Check if a file is a supported code file based on extension
///
/// Returns true for all file extensions that Julie's extractors support
fn is_code_file(path: &Path) -> bool {
    let extension = match path.extension() {
        Some(ext) => ext.to_string_lossy().to_lowercase(),
        None => return false,
    };

    // All supported language extensions
    matches!(
        extension.as_str(),
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "java"
            | "cs"
            | "php"
            | "rb"
            | "swift"
            | "kt"
            | "kts"
            | "go"
            | "c"
            | "h"
            | "cpp"
            | "cc"
            | "cxx"
            | "hpp"
            | "hxx"
            | "lua"
            | "sql"
            | "html"
            | "htm"
            | "css"
            | "scss"
            | "sass"
            | "vue"
            | "razor"
            | "cshtml"
            | "sh"
            | "bash"
            | "zsh"
            | "ps1"
            | "psm1"
            | "gd"
            | "dart"
            | "zig"
            | "qml"   // QML (Qt Modeling Language)
            | "r"     // R (Statistical Computing)
            | "md"    // Markdown
            | "markdown"
            | "json"  // JSON
            | "jsonl" // JSON Lines
            | "toml"  // TOML
            | "yml"   // YAML
            | "yaml"
    )
}
