//! Incremental indexing and orphan file cleanup
//! Handles efficient re-indexing by detecting changed files
//! Removes database entries for deleted files

use crate::handler::JulieServerHandler;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, info, trace, warn};

impl ManageWorkspaceTool {
    /// Filter files that actually need re-indexing based on hash changes
    ///
    /// Returns only files that are new, modified, or missing from database.
    /// Skips unchanged files to speed up incremental indexing.
    pub(crate) async fn filter_changed_files(
        &self,
        handler: &JulieServerHandler,
        all_files: Vec<PathBuf>,
        workspace_path: &Path,
    ) -> Result<Vec<PathBuf>> {
        // 🔥 CRITICAL DEADLOCK FIX: Generate workspace ID directly instead of registry lookup
        // Same fix as other indexing operations - avoids registry lock contention
        let workspace_id = if let Some(_workspace) = handler.get_workspace().await? {
            // CRITICAL FIX: Use the workspace_path parameter to determine canonical path
            // This ensures we get the correct workspace_id for BOTH primary and reference workspaces
            let canonical_path = workspace_path
                .canonicalize()
                .unwrap_or_else(|_| workspace_path.to_path_buf())
                .to_string_lossy()
                .to_string();

            // 🚀 DEADLOCK FIX: Generate workspace ID directly from path (no registry access)
            // This avoids the registry lock that was causing deadlocks during indexing
            match crate::workspace::registry::generate_workspace_id(&canonical_path) {
                Ok(id) => id,
                Err(e) => {
                    warn!(
                        "Failed to generate workspace ID: {} - indexing all files",
                        e
                    );
                    return Ok(all_files);
                }
            }
        } else {
            // No workspace available - all files are new
            return Ok(all_files);
        };

        // 🔥 CRITICAL FIX: Query the CORRECT database based on workspace_id
        // Primary workspace: use handler.get_workspace().db
        // Reference workspace: open its separate database at indexes/{workspace_id}/db/symbols.db
        let existing_file_hashes = if let Some(primary_workspace) = handler.get_workspace().await? {
            // Check if this is the primary workspace by comparing workspace IDs
            let primary_workspace_id = match crate::workspace::registry::generate_workspace_id(
                primary_workspace.root.to_string_lossy().as_ref(),
            ) {
                Ok(id) => id,
                Err(_) => {
                    warn!("Failed to generate primary workspace ID - treating all files as new");
                    return Ok(all_files);
                }
            };

            let is_primary = workspace_id == primary_workspace_id;
            debug!(
                "🐛 filter_changed_files: is_primary={}, workspace_id={}, primary_workspace_id={}",
                is_primary, workspace_id, primary_workspace_id
            );

            // Get the correct database based on workspace type
            let db_to_query = if is_primary {
                // Primary workspace - use handler's database connection
                primary_workspace.db.clone()
            } else {
                // Reference workspace - open its separate database
                let ref_db_path = primary_workspace.workspace_db_path(&workspace_id);
                debug!(
                    "🐛 filter_changed_files: Reference workspace DB path: {}, exists: {}",
                    ref_db_path.display(),
                    ref_db_path.exists()
                );

                if ref_db_path.exists() {
                    match tokio::task::spawn_blocking(move || {
                        crate::database::SymbolDatabase::new(ref_db_path)
                    })
                    .await
                    {
                        Ok(Ok(db)) => Some(std::sync::Arc::new(std::sync::Mutex::new(db))),
                        Ok(Err(e)) => {
                            debug!(
                                "Reference workspace DB doesn't exist yet: {} - treating all files as new",
                                e
                            );
                            return Ok(all_files);
                        }
                        Err(e) => {
                            warn!(
                                "Failed to open reference workspace DB: {} - treating all files as new",
                                e
                            );
                            return Ok(all_files);
                        }
                    }
                } else {
                    // Reference workspace database doesn't exist yet - all files are new
                    debug!("Reference workspace DB doesn't exist yet - treating all files as new");
                    return Ok(all_files);
                }
            };

            // Query the correct database
            if let Some(db) = db_to_query {
                let db_lock = match db.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!(
                            "Database mutex poisoned during file hash query, recovering: {}",
                            poisoned
                        );
                        poisoned.into_inner()
                    }
                };

                // 🔥 CRITICAL FIX: Check if THIS workspace's database has 0 symbols
                // If so, bypass incremental logic and re-index all files
                // This prevents the bug where file hashes exist but symbols were never extracted
                let symbol_count = db_lock.count_symbols_for_workspace().unwrap_or(0);
                if symbol_count == 0 {
                    info!(
                        "🔄 Workspace database has 0 symbols - bypassing incremental logic and re-indexing all {} files",
                        all_files.len()
                    );
                    drop(db_lock);
                    return Ok(all_files);
                }

                match db_lock.get_file_hashes_for_workspace() {
                    Ok(hashes) => hashes,
                    Err(e) => {
                        warn!(
                            "Failed to get existing file hashes: {} - treating all files as new",
                            e
                        );
                        return Ok(all_files);
                    }
                }
            } else {
                return Ok(all_files);
            }
        } else {
            return Ok(all_files);
        };

        debug!(
            "Checking {} files against {} existing file hashes",
            all_files.len(),
            existing_file_hashes.len()
        );

        let mut files_to_process = Vec::new();
        let mut unchanged_count = 0;
        let mut new_count = 0;
        let mut modified_count = 0;

        for file_path in &all_files {
            // Convert to relative Unix-style path for database lookup
            // Database stores paths as relative Unix-style per CLAUDE.md Path Handling Contract
            let file_path_relative =
                match crate::utils::paths::to_relative_unix_style(file_path, workspace_path) {
                    Ok(rel) => rel,
                    Err(e) => {
                        warn!(
                            "Failed to convert {} to relative path: {} - treating as new file",
                            file_path.display(),
                            e
                        );
                        files_to_process.push(file_path.clone());
                        continue;
                    }
                };

            let language = self.detect_language(file_path);

            // Calculate current file hash
            let current_hash = match crate::database::calculate_file_hash(file_path) {
                Ok(hash) => hash,
                Err(e) => {
                    warn!(
                        "Failed to calculate hash for {}: {} - including for re-indexing",
                        file_path_relative, e
                    );
                    files_to_process.push(file_path.clone());
                    continue;
                }
            };

            // Check if file exists in database and if hash matches
            if let Some(stored_hash) = existing_file_hashes.get(&file_path_relative) {
                if stored_hash == &current_hash {
                    // File unchanged by hash, but check if it needs FILE_CONTENT symbols
                    // For files without parsers (text, json, etc.), we need to ensure they have
                    // FILE_CONTENT symbols in FTS5. This is a migration for existing workspaces.

                    // Check if this is a language without a parser
                    let needs_file_content = matches!(
                        language.as_str(),
                        "text"
                            | "json"
                            | "toml"
                            | "yaml"
                            | "yml"
                            | "xml"
                            | "markdown"
                            | "md"
                            | "txt"
                            | "config"
                    );

                    if needs_file_content {
                        // Check if it has symbols (should be 0 for files without parsers)
                        if let Some(workspace) = handler.get_workspace().await? {
                            if let Some(db) = &workspace.db {
                                let db_lock = match db.lock() {
                                    Ok(guard) => guard,
                                    Err(poisoned) => {
                                        warn!(
                                            "Database mutex poisoned during symbol count check, recovering: {}",
                                            poisoned
                                        );
                                        poisoned.into_inner()
                                    }
                                };
                                let symbol_count = db_lock
                                    .get_file_symbol_count(&file_path_relative)
                                    .unwrap_or(0);
                                drop(db_lock);

                                if symbol_count == 0 {
                                    // File has no symbols - needs FILE_CONTENT symbol created
                                    debug!(
                                        "File {} has no symbols, re-indexing to create FILE_CONTENT symbol",
                                        file_path_relative
                                    );
                                    modified_count += 1;
                                    files_to_process.push(file_path.clone());
                                    continue;
                                }
                            }
                        }
                    }

                    // File truly unchanged - skip
                    unchanged_count += 1;
                } else {
                    // File modified - needs re-indexing
                    modified_count += 1;
                    files_to_process.push(file_path.clone());
                }
            } else {
                // New file - needs indexing
                new_count += 1;
                files_to_process.push(file_path.clone());
            }
        }

        info!(
            "📊 Incremental analysis: {} unchanged (skipped), {} modified, {} new - processing {} total",
            unchanged_count,
            modified_count,
            new_count,
            files_to_process.len()
        );

        // 🧹 ORPHAN CLEANUP: Remove database entries for files that no longer exist
        let orphaned_count = self
            .clean_orphaned_files(
                handler,
                &existing_file_hashes,
                &all_files,
                &workspace_id,
                workspace_path,
            )
            .await?;

        if orphaned_count > 0 {
            info!(
                "🧹 Cleaned up {} orphaned file entries from database",
                orphaned_count
            );
        }

        Ok(files_to_process)
    }

    /// Clean up orphaned database entries for files that no longer exist on disk
    ///
    /// This prevents database bloat from accumulating deleted files.
    pub(crate) async fn clean_orphaned_files(
        &self,
        handler: &JulieServerHandler,
        existing_file_hashes: &HashMap<String, String>,
        current_disk_files: &[PathBuf],
        _workspace_id: &str,
        workspace_root: &Path, // NEW: needed to convert absolute paths to relative
    ) -> Result<usize> {
        // Build set of current disk file paths for fast lookup
        // 🔥 CRITICAL FIX: Convert to relative Unix-style paths to match database format
        // Database stores relative paths like "src/helper.rs" after relative path storage contract
        let current_files: HashSet<String> = current_disk_files
            .iter()
            .filter_map(|p| {
                if p.is_absolute() {
                    crate::utils::paths::to_relative_unix_style(p, workspace_root).ok()
                } else {
                    Some(p.to_string_lossy().replace('\\', "/"))
                }
            })
            .collect();

        // Find files that are in database but not on disk (orphans)
        let orphaned_files: Vec<String> = existing_file_hashes
            .keys()
            .filter(|db_path| !current_files.contains(*db_path))
            .cloned()
            .collect();

        if orphaned_files.is_empty() {
            return Ok(0);
        }

        debug!("Found {} orphaned files to clean up", orphaned_files.len());

        // 🔥 CRITICAL FIX: Get the CORRECT database based on workspace_id
        // This function was using handler.get_workspace().db which is ALWAYS the primary workspace
        // causing reference workspace indexing to delete primary workspace files!
        let primary_workspace = match handler.get_workspace().await? {
            Some(ws) => ws,
            None => return Ok(0),
        };

        // Check if this is the primary workspace by comparing workspace IDs
        let primary_workspace_id = match crate::workspace::registry::generate_workspace_id(
            primary_workspace.root.to_string_lossy().as_ref(),
        ) {
            Ok(id) => id,
            Err(_) => {
                warn!("Failed to generate primary workspace ID");
                return Ok(0);
            }
        };

        let is_primary = _workspace_id == primary_workspace_id;

        // Get the correct database based on workspace type
        let db = if is_primary {
            // Primary workspace - use handler's database connection
            match &primary_workspace.db {
                Some(db_arc) => db_arc.clone(),
                None => return Ok(0),
            }
        } else {
            // Reference workspace - open its separate database
            let ref_db_path = primary_workspace.workspace_db_path(_workspace_id);

            debug!(
                "🗄️ Opening reference workspace DB for orphan cleanup: {}",
                ref_db_path.display()
            );

            if !ref_db_path.exists() {
                debug!("Reference workspace DB doesn't exist yet - no orphans to clean");
                return Ok(0);
            }

            match tokio::task::spawn_blocking(move || {
                crate::database::SymbolDatabase::new(ref_db_path)
            })
            .await
            {
                Ok(Ok(db)) => std::sync::Arc::new(std::sync::Mutex::new(db)),
                Ok(Err(e)) => {
                    warn!(
                        "Reference workspace DB doesn't exist yet: {} - no orphans to clean",
                        e
                    );
                    return Ok(0);
                }
                Err(e) => {
                    warn!(
                        "Failed to open reference workspace DB: {} - skipping orphan cleanup",
                        e
                    );
                    return Ok(0);
                }
            }
        };

        // 🔥 FTS5 CRITICAL FIX: Batch all deletions in ONE transaction, rebuild FTS5 ONCE
        // OLD BUG: Loop deleted files one-by-one, each triggering FTS5 rebuild (100 files = 200 rebuilds!)
        // This caused rowid desynchronization: "fts5: missing row X from content table"
        // NEW: Transaction wraps ALL deletions, single FTS5 rebuild after commit
        let mut cleaned_count = 0;
        {
            let mut db_lock = match db.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!(
                        "Database mutex poisoned during orphan cleanup, recovering: {}",
                        poisoned
                    );
                    poisoned.into_inner()
                }
            };

            // Begin transaction for ALL deletions
            db_lock.begin_transaction()?;

            for file_path in &orphaned_files {
                // Delete relationships first (referential integrity)
                let relationships_result = db_lock.conn.execute(
                    "DELETE FROM relationships WHERE from_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)
                     OR to_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
                    rusqlite::params![file_path],
                );
                if let Err(e) = relationships_result {
                    warn!(
                        "Failed to delete relationships for orphaned file {}: {}",
                        file_path, e
                    );
                    continue;
                }

                // Delete symbols (no FTS5 rebuild - happens in batch after transaction)
                let symbols_result = db_lock.conn.execute(
                    "DELETE FROM symbols WHERE file_path = ?1",
                    rusqlite::params![file_path],
                );
                if let Err(e) = symbols_result {
                    warn!(
                        "Failed to delete symbols for orphaned file {}: {}",
                        file_path, e
                    );
                    continue;
                }

                // Delete file record (CASCADE will handle any remaining symbols)
                let file_result = db_lock.conn.execute(
                    "DELETE FROM files WHERE path = ?1",
                    rusqlite::params![file_path],
                );
                if let Err(e) = file_result {
                    warn!(
                        "Failed to delete file record for orphaned file {}: {}",
                        file_path, e
                    );
                    continue;
                }

                cleaned_count += 1;
                trace!("Cleaned up orphaned file: {}", file_path);
            }

            // Commit all deletions atomically
            db_lock.commit_transaction()?;
        }

        if cleaned_count > 0 && !is_primary {
            debug!(
                "✅ Reference workspace orphan cleanup: {} files removed from workspace {}",
                cleaned_count, _workspace_id
            );
        }

        Ok(cleaned_count)
    }
}
