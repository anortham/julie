use super::super::force_safeguards::{
    cancel_embedding_tasks, refresh_workspace_ids_for_force_reindex,
};
use super::{ManageWorkspaceTool, registry_store_for_handler};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use std::path::PathBuf;
use tracing::{info, warn};

pub(crate) struct RefreshWorkspaceSuccess {
    pub(crate) workspace_id: String,
    pub(crate) workspace_path: String,
    pub(crate) status: String,
    pub(crate) files_total: usize,
    pub(crate) symbols_total: usize,
    pub(crate) relationships_total: usize,
    pub(crate) embed_outcome: crate::tools::workspace::indexing::embeddings::EmbeddingOutcome,
}

pub(crate) enum RefreshWorkspaceOutcome {
    Success(RefreshWorkspaceSuccess),
    Failure(String),
}

impl ManageWorkspaceTool {
    pub(crate) async fn refresh_workspace_internal(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
        force: bool,
    ) -> Result<RefreshWorkspaceOutcome> {
        let Some(ref db) = handler.daemon_db else {
            let message = format!(
                "Workspace refresh requires the workspace registry, which is not available in the in-process server.\n\
                 (Workspace ID: {})",
                workspace_id
            );
            return Ok(RefreshWorkspaceOutcome::Failure(message));
        };

        match db.get_workspace(workspace_id) {
            Ok(Some(ws_row)) => {
                let workspace_path = std::path::PathBuf::from(&ws_row.path);

                let guard = handler.acquire_mutation_guard(workspace_id).await;
                info!("Starting re-indexing of workspace: {}", workspace_id);

                let semantic_engine_refresh_needed = self
                    .semantic_index_engine_refresh_needed_for_path(handler, &workspace_path)
                    .await?;
                let effective_force_reindex = force || semantic_engine_refresh_needed;
                if semantic_engine_refresh_needed {
                    info!(
                        workspace_id,
                        "Index semantic version changed or missing; treating refresh as an effective full re-index"
                    );
                }
                let force_reindex_workspace_ids = if effective_force_reindex {
                    refresh_workspace_ids_for_force_reindex(workspace_id)
                } else {
                    Vec::new()
                };
                if effective_force_reindex {
                    cancel_embedding_tasks(handler, &force_reindex_workspace_ids, "refresh").await;
                }

                let index_result = self
                    .index_workspace_inner(
                        &guard,
                        handler,
                        &workspace_path,
                        effective_force_reindex,
                    )
                    .await;
                // Gate released when mutation_guard is dropped at end of this block.

                match index_result {
                    Ok(result) => {
                        if let Err(e) = db.update_workspace_stats(
                            workspace_id,
                            result.symbols_total as i64,
                            result.files_total as i64,
                            None,
                            None,
                            Some(result.duration_ms),
                        ) {
                            warn!("Failed to update workspace stats: {}", e);
                        }

                        use crate::tools::workspace::indexing::embeddings::EmbeddingOutcome;
                        let db_mutated = result.files_processed > 0 || result.orphans_cleaned > 0;
                        let embed_outcome: EmbeddingOutcome = if db_mutated
                            || effective_force_reindex
                        {
                            crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                                handler,
                                workspace_id.to_string(),
                            )
                            .await
                        } else {
                            // No files changed, but check for missing embeddings.
                            let needs_catchup = result.symbols_total > 0
                                && crate::tools::workspace::indexing::embeddings::workspace_vector_count(
                                    handler,
                                    workspace_id,
                                )
                                .await
                                    == 0;
                            let task_already_running = {
                                let tasks = handler.embedding_tasks.lock().await;
                                tasks.contains_key(workspace_id)
                            };
                            if needs_catchup && !task_already_running {
                                info!(
                                    symbols_total = result.symbols_total,
                                    "Workspace has symbols but 0 embeddings, scheduling catch-up embedding"
                                );
                                crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                                    handler,
                                    workspace_id.to_string(),
                                )
                                .await
                            } else {
                                EmbeddingOutcome { symbols: 0 }
                            }
                        };

                        let mut status = if result.files_processed == 0 {
                            "Already up-to-date.".to_string()
                        } else if effective_force_reindex {
                            format!("Full re-index: {} files processed.", result.files_processed)
                        } else {
                            format!("{} changed files re-indexed.", result.files_processed)
                        };
                        if let Some(facts_revision) = result.facts_revision {
                            status.push_str(&format!(" Canonical revision: {}.", facts_revision));
                        }

                        Ok(RefreshWorkspaceOutcome::Success(RefreshWorkspaceSuccess {
                            workspace_id: workspace_id.to_string(),
                            workspace_path: ws_row.path,
                            status,
                            files_total: result.files_total,
                            symbols_total: result.symbols_total,
                            relationships_total: result.relationships_total,
                            embed_outcome,
                        }))
                    }
                    Err(e) => {
                        let message = format!(
                            "Workspace Refresh Failed\n\
                            Workspace: {}\n\
                            Path: {}\n\
                            Error: {}\n\
                            Check that the path exists and contains readable files",
                            workspace_id, ws_row.path, e,
                        );
                        Ok(RefreshWorkspaceOutcome::Failure(message))
                    }
                }
            }
            Ok(None) => Ok(RefreshWorkspaceOutcome::Failure(format!(
                "Workspace not found: {}",
                workspace_id
            ))),
            Err(e) => Ok(RefreshWorkspaceOutcome::Failure(format!(
                "Failed to look up workspace: {}",
                e
            ))),
        }
    }

    /// Handle refresh command - re-index workspace
    pub(crate) async fn handle_refresh_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
        force: bool,
    ) -> Result<CallToolResult> {
        info!("Refreshing workspace: {}", workspace_id);

        if force && handler.current_workspace_id().as_deref() == Some(workspace_id) {
            return self.handle_index_command(handler, None, force, false).await;
        }

        match self
            .refresh_workspace_internal(handler, workspace_id, force)
            .await?
        {
            RefreshWorkspaceOutcome::Success(success) => {
                let mut message = format!(
                    "Workspace Refresh: {}\n\
                    {}\n\
                    Path: {}\n\
                    Totals: {} files, {} symbols, {} relationships",
                    success.workspace_id,
                    success.status,
                    success.workspace_path,
                    success.files_total,
                    success.symbols_total,
                    success.relationships_total,
                );
                if success.embed_outcome.symbols > 0 {
                    message.push_str(&format!(
                        "\nEmbedding {} symbols in background...",
                        success.embed_outcome.symbols
                    ));
                }
                Ok(CallToolResult::text_content(vec![Content::text(message)]))
            }
            RefreshWorkspaceOutcome::Failure(message) => {
                Ok(CallToolResult::error(vec![Content::text(message)]))
            }
        }
    }

    /// Handle rebuild command - delete the checkout's index directory and
    /// index it again from scratch under the mutation guard.
    pub(crate) async fn handle_rebuild_command(
        &self,
        handler: &JulieServerHandler,
        path: Option<String>,
        workspace_id: Option<String>,
    ) -> Result<CallToolResult> {
        let (workspace_id, root) = match (path, workspace_id) {
            (Some(path), _) => {
                let expanded = PathBuf::from(shellexpand::tilde(&path).to_string());
                let root = expanded.canonicalize().map_err(|e| {
                    anyhow::anyhow!("Failed to canonicalize workspace path '{}': {e}", path)
                })?;
                let id =
                    crate::workspace::registry::generate_workspace_id(&root.to_string_lossy())?;
                (id, root)
            }
            (None, Some(id)) => {
                let row = registry_store_for_handler(handler)?
                    .map(|store| store.get_workspace(&id))
                    .transpose()?
                    .flatten()
                    .ok_or_else(|| anyhow::anyhow!("Workspace not found: {id}"))?;
                (row.workspace_id, PathBuf::from(row.path))
            }
            (None, None) => {
                anyhow::bail!("'workspace_id' or 'path' parameter required for 'rebuild' operation")
            }
        };

        let guard = handler.acquire_mutation_guard(&workspace_id).await;
        let index_dir = handler.workspace_index_dir_for(&workspace_id).await?;
        if index_dir.exists() {
            std::fs::remove_dir_all(&index_dir)?;
            info!(
                workspace_id,
                "Deleted index directory for rebuild: {}",
                index_dir.display()
            );
        }
        handler.invalidate_checkout_store(&workspace_id).await;
        let indexed = self
            .handle_index_command_with_guard(
                handler,
                Some(root.to_string_lossy().to_string()),
                true,
                false,
                &guard,
            )
            .await?;
        let message = format!(
            "Rebuilt {}\n{}",
            root.display(),
            crate::mcp_compat::call_tool_result_text(&indexed)
        );
        Ok(if indexed.is_error.unwrap_or(false) {
            CallToolResult::error(vec![Content::text(message)])
        } else {
            CallToolResult::text_content(vec![Content::text(message)])
        })
    }
}
