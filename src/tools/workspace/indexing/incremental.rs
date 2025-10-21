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
        // Same fix as search_workspace_tantivy - avoids registry lock contention
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
                &primary_workspace.root.to_string_lossy().to_string()
            ) {
                Ok(id) => id,
                Err(_) => {
                    warn!("Failed to generate primary workspace ID - treating all files as new");
                    return Ok(all_files);
                }
            };

            let is_primary = workspace_id == primary_workspace_id;

            // Get the correct database based on workspace type
            let db_to_query = if is_primary {
                // Primary workspace - use handler's database connection
                primary_workspace.db.clone()
            } else {
                // Reference workspace - open its separate database
                let ref_db_path = primary_workspace.workspace_db_path(&workspace_id);

                if ref_db_path.exists() {
                    match tokio::task::spawn_blocking(move || {
                        crate::database::SymbolDatabase::new(ref_db_path)
                    }).await {
                        Ok(Ok(db)) => Some(std::sync::Arc::new(std::sync::Mutex::new(db))),
                        Ok(Err(e)) => {
                            debug!("Reference workspace DB doesn't exist yet: {} - treating all files as new", e);
                            return Ok(all_files);
                        }
                        Err(e) => {
                            warn!("Failed to open reference workspace DB: {} - treating all files as new", e);
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
                let db_lock = db.lock().unwrap();
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
            let file_path_str = file_path.to_string_lossy().to_string();
            let language = self.detect_language(file_path);

            // Calculate current file hash
            let current_hash = match crate::database::calculate_file_hash(file_path) {
                Ok(hash) => hash,
                Err(e) => {
                    warn!(
                        "Failed to calculate hash for {}: {} - including for re-indexing",
                        file_path_str, e
                    );
                    files_to_process.push(file_path.clone());
                    continue;
                }
            };

            // Check if file exists in database and if hash matches
            if let Some(stored_hash) = existing_file_hashes.get(&file_path_str) {
                if stored_hash == &current_hash {
                    // File unchanged by hash, but check if it needs FILE_CONTENT symbols
                    // For files without parsers (text, json, etc.), we need to ensure they have
                    // FILE_CONTENT symbols in Tantivy. This is a migration for existing workspaces.

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
                                let db_lock = db.lock().unwrap();
                                let symbol_count =
                                    db_lock.get_file_symbol_count(&file_path_str).unwrap_or(0);
                                drop(db_lock);

                                if symbol_count == 0 {
                                    // File has no symbols - needs FILE_CONTENT symbol created
                                    debug!("File {} has no symbols, re-indexing to create FILE_CONTENT symbol", file_path_str);
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
            unchanged_count, modified_count, new_count, files_to_process.len()
        );

        // 🧹 ORPHAN CLEANUP: Remove database entries for files that no longer exist
        let orphaned_count = self
            .clean_orphaned_files(handler, &existing_file_hashes, &all_files, &workspace_id)
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
    ) -> Result<usize> {
        // Build set of current disk file paths for fast lookup
        let current_files: HashSet<String> = current_disk_files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
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
            &primary_workspace.root.to_string_lossy().to_string()
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
                Some(db_arc) => db_arc,
                None => return Ok(0),
            }
        } else {
            // Reference workspace - DON'T delete from primary workspace DB!
            // This is the bug: we were comparing reference workspace files against primary DB
            // and deleting all primary files as "orphaned"
            warn!("🚨 CRITICAL BUG PREVENTED: Attempted to clean orphaned files for reference workspace using primary DB!");
            warn!("   This would have deleted all primary workspace files!");
            warn!("   Reference workspace orphan cleanup is not yet implemented - skipping.");
            // TODO: Implement reference workspace orphan cleanup by opening the correct DB
            return Ok(0);
        };

        // Delete orphaned entries
        let mut cleaned_count = 0;
        {
            let db_lock = db.lock().unwrap();

            for file_path in &orphaned_files {
                // Delete relationships first (referential integrity)
                if let Err(e) = db_lock.delete_relationships_for_file(file_path) {
                    warn!(
                        "Failed to delete relationships for orphaned file {}: {}",
                        file_path, e
                    );
                    continue;
                }

                // Delete symbols
                if let Err(e) = db_lock.delete_symbols_for_file_in_workspace(file_path) {
                    warn!(
                        "Failed to delete symbols for orphaned file {}: {}",
                        file_path, e
                    );
                    continue;
                }

                // Delete file record
                if let Err(e) = db_lock.delete_file_record_in_workspace(file_path) {
                    warn!(
                        "Failed to delete file record for orphaned file {}: {}",
                        file_path, e
                    );
                    continue;
                }

                cleaned_count += 1;
                trace!("Cleaned up orphaned file: {}", file_path);
            }
        }

        Ok(cleaned_count)
    }
}
