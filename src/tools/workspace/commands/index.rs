use super::ManageWorkspaceTool;
use super::force_safeguards::cancel_embedding_tasks;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::tools::workspace::indexing::seed::SeedReport;
use anyhow::Result;
use julie_core::workspace::mutation_gate::MutationGuard;
use std::path::Path;
use tracing::{debug, error, info, warn};

impl ManageWorkspaceTool {
    /// Handle index command - index primary workspace.
    ///
    /// Acquires the per-workspace mutation gate internally. If the caller
    /// already holds the gate (e.g. startup repair / catch-up), use
    /// `handle_index_command_with_guard` instead.
    pub(crate) async fn handle_index_command(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        force: bool,
        skip_embeddings: bool,
    ) -> Result<CallToolResult> {
        self.handle_index_command_reporting(handler, path, force, skip_embeddings)
            .await
            .map(|(result, _)| result)
    }

    /// Like `handle_index_command`, but also returns the sibling seed report
    /// when the checkout was seeded instead of indexed from scratch.
    pub(crate) async fn handle_index_command_reporting(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        force: bool,
        skip_embeddings: bool,
    ) -> Result<(CallToolResult, Option<SeedReport>)> {
        self.handle_index_command_internal(handler, path, force, skip_embeddings, None)
            .await
    }

    /// Variant for callers that already hold the workspace `MutationGuard`.
    pub(crate) async fn handle_index_command_with_guard(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        force: bool,
        skip_embeddings: bool,
        guard: &MutationGuard<'_>,
    ) -> Result<CallToolResult> {
        self.handle_index_command_internal(handler, path, force, skip_embeddings, Some(guard))
            .await
            .map(|(result, _)| result)
    }

    async fn handle_index_command_internal(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        force: bool,
        skip_embeddings: bool,
        existing_guard: Option<&MutationGuard<'_>>,
    ) -> Result<(CallToolResult, Option<SeedReport>)> {
        info!("📚 Starting workspace indexing...");
        let explicit_path_requested = path.is_some();

        let target = self.resolve_index_target(handler, path, force).await?;
        let canonical_path = target.canonical_path;
        let gate_workspace_id = target.gate_workspace_id;
        let _current_primary_root = target.current_primary_root;
        let _current_primary_id = target.current_primary_id;
        let is_non_primary_target = target.is_non_primary_target;
        let effective_force_reindex = target.effective_force_reindex;
        let force_reindex_workspace_ids = target.force_reindex_workspace_ids;

        let _local_guard;
        let _guard: &MutationGuard<'_> = match existing_guard {
            Some(guard) => guard,
            None => {
                _local_guard = handler.acquire_mutation_guard(&gate_workspace_id).await;
                &_local_guard
            }
        };

        // Clear existing state if force reindexing
        if effective_force_reindex {
            info!("🔄 Force reindex requested - clearing existing state");

            cancel_embedding_tasks(handler, &force_reindex_workspace_ids, "index").await;

            *handler.is_indexed.write().await = false;
            // Database will be cleared by initialize_workspace_with_force
        }

        let loaded_workspace = handler.get_workspace().await?;

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
            || (effective_force_reindex && !is_non_primary_workspace_target)
        {
            handler
                .initialize_workspace_with_force(
                    Some(canonical_path.to_string_lossy().to_string()),
                    effective_force_reindex,
                )
                .await?;
        } else if is_non_primary_workspace_target {
            info!("🔒 Non-primary workspace target detected, keeping handler workspace unchanged");
        }

        // Check if already indexed and not forcing reindex
        // 🔴 CRITICAL FIX: Skip this guard for non-primary workspace targets.
        // The is_indexed flag and symbol count belong to the PRIMARY workspace.
        // Without this check, calling index on a non-primary workspace path returns
        // "Workspace already indexed: {primary_symbol_count} symbols", a silent lie.
        if !effective_force_reindex && !is_non_primary_workspace_target {
            let is_indexed = *handler.is_indexed.read().await;
            if is_indexed {
                // Get symbol count from database using efficient COUNT(*) query
                let symbol_count = if let Ok(Some(workspace)) = handler.get_workspace().await {
                    workspace.store.status().graph.symbols
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

        let seeded = if effective_force_reindex {
            None
        } else {
            self.seed_if_index_missing(handler, &canonical_path, _guard)
                .await?
        };
        let (index_result, seed_report) = match seeded {
            Some((report, result)) => (Ok(result), Some(report)),
            None => (
                self.index_workspace_inner(
                    _guard,
                    handler,
                    &canonical_path,
                    effective_force_reindex,
                )
                .await,
                None,
            ),
        };

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
                    // Stdio mode: no registry, compute workspace ID for embeddings only.
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
                if let Some(facts_revision) = result.facts_revision {
                    message.push_str(&format!("\nCanonical revision: {}", facts_revision));
                }
                if let Some(report) = &seed_report {
                    message.push_str(&format!("\n{report}"));
                }
                if let Some(ws_id) = indexed_workspace_id {
                    let skip_embedding_pipeline = skip_embeddings && !effective_force_reindex;
                    if skip_embedding_pipeline {
                        info!(
                            "Skipping embeddings in auto-index mode (use explicit `manage_workspace index` to embed)"
                        );
                    } else {
                        // Only run embedding pipeline when the DB actually mutated.
                        // Matches the gate in handle_refresh_command.
                        let db_mutated = result.files_processed > 0 || result.orphans_cleaned > 0;

                        if db_mutated || effective_force_reindex {
                            let embed_outcome =
                                crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                                    handler, ws_id,
                                )
                                .await;
                            if embed_outcome.symbols > 0 {
                                message.push_str(&format!(
                                    "\nEmbedding {} symbols in background...",
                                    embed_outcome.symbols
                                ));
                            }
                        } else {
                            // No files changed, but the workspace may have been
                            // indexed before the embedding sidecar was ready.
                            // Check if symbols exist without any embeddings.
                            let embedding_count =
                                crate::tools::workspace::indexing::embeddings::workspace_vector_count(
                                    handler, &ws_id,
                                )
                                .await;

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
                                let embed_outcome =
                                    crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                                        handler, ws_id,
                                    )
                                    .await;
                                if embed_outcome.symbols > 0 {
                                    message.push_str(&format!(
                                        "\nEmbedding {} symbols in background...",
                                        embed_outcome.symbols
                                    ));
                                }
                            } else {
                                debug!("No files changed, skipping embedding pipeline");
                            }
                        }
                    }
                }
                Ok((
                    CallToolResult::text_content(vec![Content::text(message)]),
                    seed_report,
                ))
            }
            Err(e) => {
                error!("Failed to index workspace: {:#}", e);
                let message = format!(
                    "Workspace indexing failed: {:#}\nCheck that the path exists and contains source files",
                    e
                );
                Ok((CallToolResult::error(vec![Content::text(message)]), None))
            }
        }
    }

    /// Perform workspace indexing while holding the workspace mutation gate.
    ///
    /// The caller must pass the [`MutationGuard`] here as a proof token, so
    /// this function cannot run without the gate.
    pub(crate) async fn index_workspace_inner(
        &self,
        _guard: &MutationGuard<'_>,
        handler: &JulieServerHandler,
        workspace_path: &Path,
        force_reindex: bool,
    ) -> Result<crate::tools::workspace::indexing::index::IndexResult> {
        self.index_workspace_files(handler, workspace_path, force_reindex, _guard)
            .await
    }
}
