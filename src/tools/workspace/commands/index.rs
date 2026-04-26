use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, error, info, warn};

fn indexing_lock_cache() -> &'static StdMutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>> {
    static LOCKS: OnceLock<StdMutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| StdMutex::new(HashMap::new()))
}

pub(super) fn indexing_lock_for_path(path: &Path) -> Arc<AsyncMutex<()>> {
    let mut locks = match indexing_lock_cache().lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!(
                "Indexing lock cache mutex poisoned, recovering: {}",
                poisoned
            );
            poisoned.into_inner()
        }
    };

    locks
        .entry(path.to_path_buf())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

impl ManageWorkspaceTool {
    /// Handle index command - index primary workspace
    pub(crate) async fn handle_index_command(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        force: bool,
        skip_embeddings: bool,
    ) -> Result<CallToolResult> {
        info!("📚 Starting workspace indexing...");
        let explicit_path_requested = path.is_some();

        if handler.is_primary_workspace_swap_in_progress() {
            return Err(anyhow::anyhow!(
                "Primary workspace identity unavailable during swap"
            ));
        }

        let loaded_workspace = handler.get_workspace().await?;
        let current_primary_root = if explicit_path_requested || loaded_workspace.is_none() {
            handler.current_workspace_root()
        } else {
            handler.require_primary_workspace_root()?
        };
        let current_primary_id = handler.current_workspace_id().or_else(|| {
            crate::workspace::registry::generate_workspace_id(
                &current_primary_root.to_string_lossy(),
            )
            .ok()
        });
        let bound_primary_id = handler.current_workspace_id();

        // Get the original path before deciding whether this targets a non-primary workspace.
        // Uses the session-owned current primary root as the authoritative fallback.
        // resolved in main.rs from CLI --workspace > JULIE_WORKSPACE env > current_dir.
        let original_path = match path {
            Some(ref p) => {
                let expanded = shellexpand::tilde(p).to_string();
                PathBuf::from(expanded)
            }
            None => current_primary_root.clone(),
        };

        // 🔥 CRITICAL FIX: Check if this targets a non-primary workspace FIRST before calling find_workspace_root.
        // Those workspaces do not have .julie/ directories, so find_workspace_root will walk up
        // to the primary workspace and return the wrong path!
        let is_non_primary_target = if let Some(ref db) = handler.daemon_db {
            // Daemon mode: registered but not the primary workspace.
            if let Some(ref primary_id) = bound_primary_id {
                db.get_workspace_by_path(original_path.to_string_lossy().as_ref())
                    .ok()
                    .flatten()
                    .map(|row| row.workspace_id != *primary_id)
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            // Stdio mode: if workspace is already loaded and the requested path
            // is outside the current primary root, treat it as non-primary. Without this,
            // resolve_workspace_path walks up to the primary's markers (e.g. .git)
            // and conflates the target path with the primary workspace.
            let primary_canonical = current_primary_root
                .canonicalize()
                .unwrap_or_else(|_| current_primary_root.clone());
            let request_canonical = original_path
                .canonicalize()
                .unwrap_or_else(|_| original_path.clone());
            request_canonical != primary_canonical
                && !request_canonical.starts_with(&primary_canonical)
        };

        // For non-primary targets, use the original path directly (no workspace root resolution)
        // For primary workspaces, resolve to workspace root using markers
        let workspace_path = if is_non_primary_target {
            debug!("Non-primary workspace target detected, using original path directly");
            original_path.clone()
        } else {
            self.resolve_workspace_path(path, Some(&current_primary_root))?
        };

        let canonical_path = workspace_path
            .canonicalize()
            .unwrap_or_else(|_| workspace_path.clone());

        let index_lock = indexing_lock_for_path(&canonical_path);

        let _index_guard = index_lock.lock().await;
        let force_reindex = force;

        info!("🎯 Resolved workspace path: {}", canonical_path.display());

        // Clear existing state if force reindexing
        if force_reindex {
            info!("🔄 Force reindex requested - clearing existing state");

            // Cancel any running embedding pipeline FIRST, before touching the DB.
            // This prevents GPU errors from concurrent DB access and avoids the
            // race where a running pipeline writes embeddings back after we clear.
            // Use the target workspace_id, which may differ from the primary during force reindex.
            let cancel_ws_id =
                crate::workspace::registry::generate_workspace_id(&original_path.to_string_lossy())
                    .ok()
                    .or_else(|| current_primary_id.clone());
            if let Some(ref ws_id) = cancel_ws_id {
                let mut tasks = handler.embedding_tasks.lock().await;
                if let Some((cancel_flag, handle)) = tasks.remove(ws_id) {
                    info!("🛑 Cancelling running embedding pipeline for workspace {ws_id}");
                    cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    handle.abort();
                }
            }

            *handler.is_indexed.write().await = false;
            // Database will be cleared by initialize_workspace_with_force
        }

        // 🔥 CRITICAL FIX: Only initialize the handler when indexing the primary workspace.
        // Non-primary workspace targets should never reinitialize handler.workspace.
        // They are indexed into the primary workspace's indexes/{workspace_id}/ directory
        let workspace_already_loaded = loaded_workspace.is_some();
        let loaded_workspace_matches_target =
            loaded_workspace.as_ref().map_or(false, |workspace| {
                let loaded_root = workspace
                    .root
                    .canonicalize()
                    .unwrap_or_else(|_| workspace.root.clone());
                loaded_root == canonical_path
            });

        let is_non_primary_workspace_target = is_non_primary_target;

        // Only initialize if:
        // 1. Workspace not loaded yet, OR
        // 2. Current primary target differs from the loaded workspace, OR
        // 3. Forcing reindex AND this is NOT a non-primary workspace target
        if !workspace_already_loaded
            || (!is_non_primary_workspace_target && !loaded_workspace_matches_target)
            || (force_reindex && !is_non_primary_workspace_target)
        {
            handler
                .initialize_workspace_with_force(
                    Some(canonical_path.to_string_lossy().to_string()),
                    force_reindex,
                )
                .await?;
        } else if is_non_primary_workspace_target {
            info!("🔒 Non-primary workspace target detected, keeping handler workspace unchanged");
        }

        // Check if already indexed and not forcing reindex
        // 🔴 CRITICAL FIX: Skip this guard for non-primary workspace targets.
        // The is_indexed flag and symbol count belong to the PRIMARY workspace.
        // Without this check, calling index on a non-primary workspace path returns
        // "Workspace already indexed: {primary_symbol_count} symbols" — a silent lie.
        if !force_reindex && !is_non_primary_workspace_target {
            let is_indexed = *handler.is_indexed.read().await;
            if is_indexed {
                // Get symbol count from database using efficient COUNT(*) query
                let symbol_count = if let Ok(Some(workspace)) = handler.get_workspace().await {
                    if let Some(db) = workspace.db.as_ref() {
                        let db_lock = match db.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => {
                                warn!(
                                    "Database mutex poisoned during symbol count, recovering: {}",
                                    poisoned
                                );
                                poisoned.into_inner()
                            }
                        };
                        // OPTIMIZED: Use SQL COUNT(*) instead of loading all symbols
                        db_lock.count_symbols_for_workspace().unwrap_or(0)
                    } else {
                        0
                    }
                } else {
                    0
                };

                // 🔥 CRITICAL FIX: If database is empty, clear the flag and proceed with indexing
                // This prevents the nonsensical "Workspace already indexed: 0 symbols" message
                if symbol_count == 0 {
                    warn!(
                        "is_indexed flag was true but database has 0 symbols - clearing flag and proceeding with indexing"
                    );
                    *handler.is_indexed.write().await = false;
                    // Fall through to indexing logic below
                } else {
                    // Workspace has symbols. Run incremental indexing to catch
                    // files that changed while the daemon was down. The blake3
                    // hash comparison in filter_changed_files is fast when
                    // nothing changed (just reads hashes from the DB).
                    info!(
                        "Workspace has {} symbols, running incremental update",
                        symbol_count
                    );

                    // Ensure daemon.db status reflects reality.
                    let final_current_primary_id = if explicit_path_requested {
                        crate::workspace::registry::generate_workspace_id(
                            &canonical_path.to_string_lossy(),
                        )?
                    } else {
                        handler.require_primary_workspace_identity()?
                    };
                    if let Some(ref daemon_db) = handler.daemon_db {
                        let _ =
                            daemon_db.update_workspace_status(&final_current_primary_id, "ready");
                    }
                    // Fall through to index_workspace_files with force=false.
                    // The incremental pipeline will hash-compare and only
                    // re-index changed files.
                }
            }
        }

        // Fix C part c: pause the target workspace's watcher during force reindex
        // to prevent the watcher from dispatching concurrent incremental updates to
        // the same target DB while the full reindex is running.
        let target_watcher_id: Option<String> = if is_non_primary_workspace_target && force_reindex
        {
            let path_str = canonical_path.to_string_lossy().to_string();
            crate::workspace::registry::generate_workspace_id(&path_str).ok()
        } else {
            None
        };
        if let (Some(id), Some(pool)) = (&target_watcher_id, &handler.watcher_pool) {
            pool.pause_workspace(id).await;
        }

        // Perform indexing
        let index_result = self
            .index_workspace_files(handler, &canonical_path, force_reindex)
            .await;

        // Resume the target watcher before handling the result, whether Ok or Err.
        if let (Some(id), Some(pool)) = (&target_watcher_id, &handler.watcher_pool) {
            pool.resume_workspace(id).await;
        }

        match index_result {
            Ok(result) => {
                let files_total = result.files_total;
                let symbols_total = result.symbols_total;
                let relationships_total = result.relationships_total;

                // Mark as indexed
                *handler.is_indexed.write().await = true;

                // Register/update workspace stats and resolve workspace ID for embeddings
                let mut indexed_workspace_id: Option<String> = None;
                let canonical_path_str = canonical_path.to_string_lossy().to_string();

                if let Some(ref daemon_db) = handler.daemon_db {
                    let final_current_primary_id = if explicit_path_requested {
                        crate::workspace::registry::generate_workspace_id(&canonical_path_str)?
                    } else {
                        handler.require_primary_workspace_identity()?
                    };
                    // Daemon mode: persist stats to daemon.db.
                    // Fix A: non-primary workspaces must derive workspace_id from their own path,
                    // NOT from handler.workspace_id (which belongs to the primary workspace).
                    let workspace_id = if is_non_primary_workspace_target {
                        crate::workspace::registry::generate_workspace_id(&canonical_path_str)
                            .unwrap_or_default()
                    } else {
                        final_current_primary_id
                    };
                    let _ = daemon_db.upsert_workspace(&workspace_id, &canonical_path_str, "ready");
                    let _ = daemon_db.update_workspace_stats(
                        &workspace_id,
                        symbols_total as i64,
                        files_total as i64,
                        None,
                        None,
                        Some(result.duration_ms),
                    );
                    info!(
                        "✅ Updated daemon.db stats: {} files, {} symbols for {}",
                        files_total, symbols_total, workspace_id
                    );
                    indexed_workspace_id = Some(workspace_id);
                } else {
                    // Stdio mode: no registry — compute workspace ID for embeddings only
                    if let Ok(ws_id) =
                        crate::workspace::registry::generate_workspace_id(&canonical_path_str)
                    {
                        indexed_workspace_id = Some(ws_id);
                    }
                }

                let mut message = format!(
                    "Workspace indexing complete: {} files, {} symbols, {} relationships\nReady for search and navigation",
                    files_total, symbols_total, relationships_total
                );
                if let Some(canonical_revision) = result.canonical_revision {
                    message.push_str(&format!("\nCanonical revision: {}", canonical_revision));
                }
                if let Some(ws_id) = indexed_workspace_id {
                    if skip_embeddings {
                        info!(
                            "Skipping embeddings in auto-index mode (use explicit `manage_workspace index` to embed)"
                        );
                    } else {
                        // Only run embedding pipeline when the DB actually mutated.
                        // Matches the gate in handle_refresh_command.
                        let db_mutated = result.files_processed > 0 || result.orphans_cleaned > 0;

                        if db_mutated || force {
                            // Force re-index: pipeline was already cancelled at the top
                            // of this function. Clear embeddings so the new pipeline
                            // re-embeds everything with the latest enrichment text.
                            //
                            // Bug fix: route the clear to the CORRECT workspace DB.
                            // handler.get_workspace().db always points to the PRIMARY
                            // workspace. For non-primary targets we must open the
                            // target DB via workspace_db_path() instead.
                            if force {
                                if is_non_primary_workspace_target {
                                    let target_db_path =
                                        handler.workspace_db_file_path_for(&ws_id).await?;
                                    if target_db_path.exists() {
                                        let path = target_db_path;
                                        let clear_result = tokio::task::spawn_blocking(move || {
                                            let mut target_db =
                                                crate::database::SymbolDatabase::new(path)?;
                                            target_db.clear_all_embeddings()
                                        })
                                        .await;
                                        match clear_result {
                                            Ok(Ok(())) => info!(
                                                "🗑️ Cleared target workspace embeddings for force re-embed"
                                            ),
                                            Ok(Err(e)) => tracing::warn!(
                                                "Failed to clear target-workspace embeddings: {e}"
                                            ),
                                            Err(e) => tracing::warn!(
                                                "Target-workspace embedding clear task panicked: {e}"
                                            ),
                                        }
                                    } else {
                                        debug!(
                                            "Target DB does not exist at {}, nothing to clear",
                                            target_db_path.display()
                                        );
                                    }
                                } else if let Ok(Some(workspace)) = handler.get_workspace().await {
                                    if let Some(ref db) = workspace.db {
                                        // Primary workspace: clear from the handler's workspace DB.
                                        let mut db_lock =
                                            db.lock().unwrap_or_else(|p| p.into_inner());
                                        match db_lock.clear_all_embeddings() {
                                            Ok(()) => info!(
                                                "🗑️ Cleared all embeddings for force re-embed"
                                            ),
                                            Err(e) => {
                                                tracing::warn!("Failed to clear embeddings: {e}")
                                            }
                                        }
                                    }
                                }
                            }

                            let embed_count =
                                crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                                    handler, ws_id,
                                )
                                .await;
                            if embed_count > 0 {
                                message.push_str(&format!(
                                    "\nEmbedding {} symbols in background...",
                                    embed_count
                                ));
                            }
                        } else {
                            // No files changed, but the workspace may have been
                            // indexed before the embedding sidecar was ready.
                            // Check if symbols exist without any embeddings.
                            let embedding_count = if is_non_primary_workspace_target {
                                match handler.workspace_db_file_path_for(&ws_id).await {
                                    Ok(path) if path.exists() => {
                                        let c = tokio::task::spawn_blocking(move || {
                                            crate::database::SymbolDatabase::new(path)
                                                .and_then(|db| db.embedding_count())
                                                .unwrap_or(0)
                                        })
                                        .await
                                        .unwrap_or(0);
                                        c
                                    }
                                    _ => 0,
                                }
                            } else if let Ok(Some(ws)) = handler.get_workspace().await {
                                ws.db.as_ref().map_or(0, |db| {
                                    db.lock()
                                        .unwrap_or_else(|p| p.into_inner())
                                        .embedding_count()
                                        .unwrap_or(0)
                                })
                            } else {
                                0
                            };

                            // Skip catch-up if an embedding task is already
                            // running (it may not have stored its first batch
                            // yet, so embedding_count is still 0).
                            let task_already_running = {
                                let tasks = handler.embedding_tasks.lock().await;
                                tasks.contains_key(&ws_id)
                            };

                            if embedding_count == 0 && symbols_total > 0 && !task_already_running {
                                info!(
                                    symbols_total,
                                    "Workspace has symbols but 0 embeddings, scheduling catch-up embedding"
                                );
                                let embed_count =
                                    crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                                        handler, ws_id,
                                    )
                                    .await;
                                if embed_count > 0 {
                                    message.push_str(&format!(
                                        "\nEmbedding {} symbols in background...",
                                        embed_count
                                    ));
                                }
                            } else {
                                debug!("No files changed, skipping embedding pipeline");
                            }
                        }
                    }
                }
                Ok(CallToolResult::text_content(vec![Content::text(message)]))
            }
            Err(e) => {
                error!("Failed to index workspace: {}", e);
                let message = format!(
                    "Workspace indexing failed: {}\nCheck that the path exists and contains source files",
                    e
                );
                Ok(CallToolResult::text_content(vec![Content::text(message)]))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::indexing_lock_for_path;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn test_shared_index_lock_reuses_lock_for_same_path() {
        let path = PathBuf::from("/tmp/julie-shared-lock");

        let first = indexing_lock_for_path(&path);
        let second = indexing_lock_for_path(&path);

        assert!(
            Arc::ptr_eq(&first, &second),
            "same canonical path should reuse the same indexing lock"
        );
    }
}
