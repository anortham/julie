//! Incremental indexing and orphan file cleanup
//! Handles efficient re-indexing by detecting changed files
//! Removes database entries for deleted files

use super::route::IndexRoute;
use crate::database::ProjectionStatus;
use crate::handler::JulieServerHandler;
use crate::search::projection::TANTIVY_PROJECTION_NAME;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tracing::{debug, info, trace, warn};

impl ManageWorkspaceTool {
    /// Filter files that actually need re-indexing based on hash changes
    ///
    /// Returns (files_to_process, orphans_cleaned) where orphans_cleaned is the
    /// count of database entries removed for files that no longer exist on disk.
    pub(crate) async fn filter_changed_files(
        &self,
        handler: &JulieServerHandler,
        all_files: Vec<PathBuf>,
        route: &IndexRoute,
    ) -> Result<(Vec<PathBuf>, usize)> {
        let workspace_id = route.workspace_id.clone();

        let Some(db) = route.database_for_read(handler).await? else {
            return Ok((all_files, 0));
        };
        debug!(
            "🐛 filter_changed_files: is_primary={}, workspace_id={}, db_path={}",
            route.is_primary,
            workspace_id,
            route.db_path.display()
        );

        let existing_file_hashes = {
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

            let symbol_count = db_lock.count_symbols_for_workspace().unwrap_or(0);
            if symbol_count == 0 {
                info!(
                    "🔄 Workspace database has 0 symbols - bypassing incremental logic and re-indexing all {} files",
                    all_files.len()
                );
                drop(db_lock);
                return Ok((all_files, 0));
            }

            match db_lock.get_file_hashes_for_workspace() {
                Ok(hashes) => hashes,
                Err(e) => {
                    warn!(
                        "Failed to get existing file hashes: {} - treating all files as new",
                        e
                    );
                    return Ok((all_files, 0));
                }
            }
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
                match crate::utils::paths::to_relative_unix_style(file_path, &route.workspace_root)
                {
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
            .clean_orphaned_files(handler, &existing_file_hashes, &all_files, route)
            .await?;

        if orphaned_count > 0 {
            info!(
                "🧹 Cleaned up {} orphaned file entries from database",
                orphaned_count
            );
        }

        Ok((files_to_process, orphaned_count))
    }

    /// Clean up orphaned database entries for files that no longer exist on disk
    ///
    /// This prevents database bloat from accumulating deleted files.
    pub(crate) async fn clean_orphaned_files(
        &self,
        handler: &JulieServerHandler,
        existing_file_hashes: &HashMap<String, String>,
        current_disk_files: &[PathBuf],
        route: &IndexRoute,
    ) -> Result<usize> {
        // Build set of current disk file paths for fast lookup
        // 🔥 CRITICAL FIX: Convert to relative Unix-style paths to match database format
        // Database stores relative paths like "src/helper.rs" after relative path storage contract
        let current_files: HashSet<String> = current_disk_files
            .iter()
            .filter_map(|p| {
                if p.is_absolute() {
                    crate::utils::paths::to_relative_unix_style(p, &route.workspace_root).ok()
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

        let search_index = route.search_index_for_write().await?;

        let Some(db) = route.database_for_write(handler).await? else {
            return Ok(0);
        };

        let (cleaned_count, canonical_revision) = {
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

            let projected_revision = db_lock
                .get_projection_state(TANTIVY_PROJECTION_NAME, &route.workspace_id)?
                .and_then(|state| {
                    state.projected_revision.or_else(|| {
                        if state.status == ProjectionStatus::Ready {
                            state.canonical_revision
                        } else {
                            None
                        }
                    })
                });

            let canonical_revision =
                db_lock.delete_orphaned_files_atomic(&route.workspace_id, &orphaned_files)?;
            let cleaned_count = orphaned_files.len();

            if let Some(revision) = canonical_revision {
                db_lock.upsert_projection_state(
                    TANTIVY_PROJECTION_NAME,
                    &route.workspace_id,
                    ProjectionStatus::Stale,
                    Some(revision),
                    projected_revision,
                    Some("orphan cleanup committed to SQLite; Tantivy cleanup pending"),
                )?;
            }

            for file_path in &orphaned_files {
                trace!("Cleaned up orphaned file: {}", file_path);
            }
            (cleaned_count, canonical_revision)
        };

        let mut tantivy_synced = false;
        if let Some(ref search_idx) = search_index {
            match search_idx.lock() {
                Ok(idx) => {
                    let mut remove_failed = false;
                    for file_path in &orphaned_files {
                        if let Err(e) = idx.remove_by_file_path(file_path) {
                            warn!(
                                "Failed to remove Tantivy docs for orphaned file {}: {}",
                                file_path, e
                            );
                            remove_failed = true;
                        }
                    }
                    if let Err(e) = idx.commit() {
                        warn!("Failed to commit Tantivy after orphan cleanup: {}", e);
                    } else {
                        tantivy_synced = !remove_failed;
                    }
                }
                Err(e) => {
                    warn!("Tantivy index mutex poisoned during orphan cleanup: {}", e);
                }
            }
        }

        if tantivy_synced {
            if let Some(revision) = canonical_revision {
                let db_lock = match db.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        warn!(
                            "Database mutex poisoned while finalizing orphan cleanup state: {}",
                            poisoned
                        );
                        poisoned.into_inner()
                    }
                };
                db_lock.upsert_projection_state(
                    TANTIVY_PROJECTION_NAME,
                    &route.workspace_id,
                    ProjectionStatus::Ready,
                    Some(revision),
                    Some(revision),
                    None,
                )?;
            }
        }

        if cleaned_count > 0 && !route.is_primary {
            debug!(
                "✅ Reference workspace orphan cleanup: {} files removed from workspace {}",
                cleaned_count, route.workspace_id
            );
        }

        Ok(cleaned_count)
    }
}
