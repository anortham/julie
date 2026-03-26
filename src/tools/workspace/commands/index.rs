use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, error, info, warn};

fn indexing_lock_cache() -> &'static StdMutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>> {
    static LOCKS: OnceLock<StdMutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| StdMutex::new(HashMap::new()))
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
            false
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

        let index_lock = {
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
                .entry(canonical_path.clone())
                .or_insert_with(|| Arc::new(AsyncMutex::new(())))
                .clone()
        };

        let _index_guard = index_lock.lock().await;
        let force_reindex = force;

        info!("🎯 Resolved workspace path: {}", canonical_path.display());

        // Clear existing state if force reindexing
        if force_reindex {
            info!("🔄 Force reindex requested - clearing existing state");

            // Cancel any running embedding pipeline FIRST, before touching the DB.
            // This prevents GPU errors from concurrent DB access and avoids the
            // race where a running pipeline writes embeddings back after we clear.
            {
                let mut task_guard = handler.embedding_task.lock().await;
                if let Some((cancel_flag, handle)) = task_guard.take() {
                    info!("🛑 Cancelling running embedding pipeline for force re-index");
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

        // Check if this path is a reference workspace (check ORIGINAL path, not resolved path!)
        let is_reference_workspace = if let Some(ref db) = handler.daemon_db {
            // Daemon mode: registered but not the primary workspace → reference
            if let Some(ref primary_id) = handler.workspace_id {
                match db.get_workspace_by_path(original_path.to_string_lossy().as_ref()) {
                    Ok(Some(row)) => {
                        let is_ref = row.workspace_id != *primary_id;
                        debug!(
                            "Found in daemon.db - workspace_id: {}, is_reference: {}",
                            row.workspace_id, is_ref
                        );
                        is_ref
                    }
                    Ok(None) => {
                        debug!("Path not found in daemon.db");
                        false
                    }
                    Err(e) => {
                        debug!("Error checking daemon.db: {}", e);
                        false
                    }
                }
            } else {
                debug!("No primary workspace ID");
                false
            }
        } else {
            debug!("No daemon.db - stdio mode");
            false
        };

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
                    // Resume incomplete embedding if needed. The pipeline is
                    // incremental (skips already-embedded symbols) and fast-exits
                    // when everything is already embedded. This handles the case
                    // where the daemon was killed mid-embedding and restarted.
                    let mut message = format!(
                        "Workspace already indexed: {} symbols\nUse force: true to re-index",
                        symbol_count
                    );
                    if !skip_embeddings {
                        let canonical_path_str = canonical_path.to_string_lossy().to_string();
                        let ws_id = handler.workspace_id.clone().unwrap_or_else(|| {
                            crate::workspace::registry::generate_workspace_id(&canonical_path_str)
                                .unwrap_or_default()
                        });
                        if !ws_id.is_empty() {
                            let embed_count =
                                crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                                    handler, ws_id,
                                )
                                .await;
                            if embed_count > 0 {
                                message.push_str(&format!(
                                    "\nResuming embedding for {} symbols in background...",
                                    embed_count
                                ));
                            }
                        }
                    }
                    // Ensure daemon.db status reflects reality. The workspace
                    // pool's get_or_init always upserts with "pending"; without
                    // this, already-indexed workspaces stay "pending" forever
                    // after a daemon restart.
                    if let Some(ref daemon_db) = handler.daemon_db {
                        if let Some(ref ws_id) = handler.workspace_id {
                            let _ = daemon_db.update_workspace_status(ws_id, "ready");
                        }
                    }
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            }
        }

        // Perform indexing
        match self
            .index_workspace_files(handler, &canonical_path, force_reindex)
            .await
        {
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
                    // Daemon mode: persist stats to daemon.db
                    let workspace_id = handler.workspace_id.clone().unwrap_or_else(|| {
                        crate::workspace::registry::generate_workspace_id(&canonical_path_str)
                            .unwrap_or_default()
                    });
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
                        // Force re-index: pipeline was already cancelled at the top
                        // of this function. Clear embeddings so the new pipeline
                        // re-embeds everything with the latest enrichment text.
                        if force {
                            if let Ok(Some(workspace)) = handler.get_workspace().await {
                                if let Some(ref db) = workspace.db {
                                    let mut db_lock = db.lock().unwrap_or_else(|p| p.into_inner());
                                    match db_lock.clear_all_embeddings() {
                                        Ok(()) => {
                                            info!("🗑️ Cleared all embeddings for force re-embed")
                                        }
                                        Err(e) => tracing::warn!("Failed to clear embeddings: {e}"),
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
