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

        // Get original path for reference workspace check BEFORE resolution.
        // Uses handler.workspace_root as the authoritative fallback — it was already
        // resolved in main.rs from CLI --workspace > JULIE_WORKSPACE env > current_dir.
        let original_path = match path {
            Some(ref p) => {
                let expanded = shellexpand::tilde(p).to_string();
                PathBuf::from(expanded)
            }
            None => handler.workspace_root.clone(),
        };

        // 🔥 CRITICAL FIX: Check if this is a reference workspace FIRST before calling find_workspace_root
        // Reference workspaces don't have .julie/ directories, so find_workspace_root will walk up
        // to the primary workspace and return the wrong path!
        let is_reference_check = if let Some(ref db) = handler.daemon_db {
            // Daemon mode: registered but not the primary workspace → treat as reference
            if let Some(ref primary_id) = handler.workspace_id {
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
            // differs from the primary root, treat as reference. Without this,
            // resolve_workspace_path walks up to the primary's markers (e.g. .git)
            // and conflates the reference path with the primary workspace.
            let primary_canonical = handler
                .workspace_root
                .canonicalize()
                .unwrap_or_else(|_| handler.workspace_root.clone());
            let request_canonical = original_path
                .canonicalize()
                .unwrap_or_else(|_| original_path.clone());
            request_canonical != primary_canonical
        };

        // For reference workspaces, use the original path directly (no workspace root resolution)
        // For primary workspaces, resolve to workspace root using markers
        let workspace_path = if is_reference_check {
            debug!("Reference workspace detected - using original path directly");
            original_path.clone()
        } else {
            self.resolve_workspace_path(path, Some(&handler.workspace_root))?
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
            // Use the TARGET workspace_id (may differ from primary when force-reindexing
            // a reference workspace).
            let cancel_ws_id =
                crate::workspace::registry::generate_workspace_id(&original_path.to_string_lossy())
                    .ok()
                    .or_else(|| handler.workspace_id.clone());
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

        // 🔥 CRITICAL FIX: Only initialize workspace if it's the PRIMARY workspace being indexed
        // Reference workspaces should NEVER reinitialize the handler's workspace!
        // They are indexed into the primary workspace's indexes/{workspace_id}/ directory
        let workspace_already_loaded = handler.get_workspace().await?.is_some();

        let is_reference_workspace = is_reference_check;

        // Only initialize if:
        // 1. Workspace not loaded yet, OR
        // 2. Forcing reindex AND this is NOT a reference workspace
        if !workspace_already_loaded || (force_reindex && !is_reference_workspace) {
            handler
                .initialize_workspace_with_force(
                    Some(canonical_path.to_string_lossy().to_string()),
                    force_reindex,
                )
                .await?;
        } else if is_reference_workspace {
            info!("🔒 Reference workspace detected - keeping handler workspace unchanged");
        }

        // Check if already indexed and not forcing reindex
        // 🔴 CRITICAL FIX: Skip this guard for reference workspaces!
        // The is_indexed flag and symbol count belong to the PRIMARY workspace.
        // Without this check, calling index on a reference workspace path returns
        // "Workspace already indexed: {primary_symbol_count} symbols" — a silent lie.
        if !force_reindex && !is_reference_workspace {
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
                    if let Some(ref daemon_db) = handler.daemon_db {
                        if let Some(ref ws_id) = handler.workspace_id {
                            let _ = daemon_db.update_workspace_status(ws_id, "ready");
                        }
                    }
                    // Fall through to index_workspace_files with force=false.
                    // The incremental pipeline will hash-compare and only
                    // re-index changed files.
                }
            }
        }

        // Fix C part c: pause the reference workspace's watcher during force reindex
        // to prevent the watcher from dispatching concurrent incremental updates to
        // the same reference DB while the full reindex is running.
        let ref_watcher_id: Option<String> = if is_reference_workspace && force_reindex {
            let path_str = canonical_path.to_string_lossy().to_string();
            crate::workspace::registry::generate_workspace_id(&path_str).ok()
        } else {
            None
        };
        if let (Some(id), Some(pool)) = (&ref_watcher_id, &handler.watcher_pool) {
            pool.pause_workspace(id).await;
        }

        // Perform indexing
        let index_result = self
            .index_workspace_files(handler, &canonical_path, force_reindex)
            .await;

        // Resume reference watcher before handling the result (whether Ok or Err).
        if let (Some(id), Some(pool)) = (&ref_watcher_id, &handler.watcher_pool) {
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
                    // Daemon mode: persist stats to daemon.db.
                    // Fix A: reference workspaces must derive workspace_id from their own path,
                    // NOT from handler.workspace_id (which belongs to the primary workspace).
                    let workspace_id = if is_reference_workspace {
                        crate::workspace::registry::generate_workspace_id(&canonical_path_str)
                            .unwrap_or_default()
                    } else {
                        handler.workspace_id.clone().unwrap_or_else(|| {
                            crate::workspace::registry::generate_workspace_id(&canonical_path_str)
                                .unwrap_or_default()
                        })
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
                            // workspace. For reference workspaces we must open the
                            // reference DB via workspace_db_path() instead.
                            if force {
                                if let Ok(Some(workspace)) = handler.get_workspace().await {
                                    if is_reference_workspace {
                                        // Open the REFERENCE workspace DB directly.
                                        // handler.get_workspace().db is the PRIMARY, not the reference.
                                        let ref_db_path = workspace.workspace_db_path(&ws_id);
                                        if ref_db_path.exists() {
                                            let path = ref_db_path;
                                            let clear_result =
                                                tokio::task::spawn_blocking(move || {
                                                    let mut ref_db =
                                                        crate::database::SymbolDatabase::new(path)?;
                                                    ref_db.clear_all_embeddings()
                                                })
                                                .await;
                                            match clear_result {
                                                Ok(Ok(())) => info!(
                                                    "🗑️ Cleared reference workspace embeddings for force re-embed"
                                                ),
                                                Ok(Err(e)) => tracing::warn!(
                                                    "Failed to clear reference embeddings: {e}"
                                                ),
                                                Err(e) => tracing::warn!(
                                                    "Reference embedding clear task panicked: {e}"
                                                ),
                                            }
                                        } else {
                                            debug!(
                                                "Reference DB does not exist at {}, nothing to clear",
                                                ref_db_path.display()
                                            );
                                        }
                                    } else if let Some(ref db) = workspace.db {
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
                            debug!("No files changed, skipping embedding pipeline");
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
