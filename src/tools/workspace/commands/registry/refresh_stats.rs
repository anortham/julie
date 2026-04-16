use super::super::index::indexing_lock_for_path;
use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::{info, warn};

pub(crate) struct RefreshWorkspaceSuccess {
    pub(crate) workspace_id: String,
    pub(crate) workspace_path: String,
    pub(crate) status: String,
    pub(crate) files_total: usize,
    pub(crate) symbols_total: usize,
    pub(crate) relationships_total: usize,
    pub(crate) embed_count: usize,
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
    ) -> Result<RefreshWorkspaceOutcome> {
        let Some(ref db) = handler.daemon_db else {
            let message = format!(
                "Workspace refresh requires daemon mode. Start the daemon with `julie daemon`.\n\
                 (Workspace ID: {})",
                workspace_id
            );
            return Ok(RefreshWorkspaceOutcome::Failure(message));
        };

        match db.get_workspace(workspace_id) {
            Ok(Some(ws_row)) => {
                let workspace_path = std::path::PathBuf::from(&ws_row.path);
                let canonical_path = workspace_path
                    .canonicalize()
                    .unwrap_or_else(|_| workspace_path.clone());
                let index_lock = indexing_lock_for_path(&canonical_path);
                let _index_guard = index_lock.lock().await;
                info!("Starting re-indexing of workspace: {}", workspace_id);

                let force = self.force.unwrap_or(false);
                let current_primary_id = handler.current_workspace_id();
                let ref_watcher_id = if force && current_primary_id.as_deref() != Some(workspace_id)
                {
                    Some(workspace_id.to_string())
                } else {
                    None
                };
                if let (Some(id), Some(pool)) = (&ref_watcher_id, &handler.watcher_pool) {
                    pool.pause_workspace(id).await;
                }

                let index_result = self
                    .index_workspace_files(handler, &workspace_path, force)
                    .await;

                if let (Some(id), Some(pool)) = (&ref_watcher_id, &handler.watcher_pool) {
                    pool.resume_workspace(id).await;
                }

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

                        let db_mutated = result.files_processed > 0 || result.orphans_cleaned > 0;
                        let embed_count = if db_mutated || force {
                            if force {
                                let mut tasks = handler.embedding_tasks.lock().await;
                                if let Some((cancel_flag, handle)) = tasks.remove(workspace_id) {
                                    info!(
                                        "Cancelling running embedding pipeline for force refresh"
                                    );
                                    cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                                    handle.abort();
                                }
                            }

                            crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                                handler,
                                workspace_id.to_string(),
                            )
                            .await
                        } else {
                            0
                        };

                        let mut status = if result.files_processed == 0 {
                            "Already up-to-date.".to_string()
                        } else if force {
                            format!("Full re-index: {} files processed.", result.files_processed)
                        } else {
                            format!("{} changed files re-indexed.", result.files_processed)
                        };
                        if let Some(canonical_revision) = result.canonical_revision {
                            status
                                .push_str(&format!(" Canonical revision: {}.", canonical_revision));
                        }

                        Ok(RefreshWorkspaceOutcome::Success(RefreshWorkspaceSuccess {
                            workspace_id: workspace_id.to_string(),
                            workspace_path: ws_row.path,
                            status,
                            files_total: result.files_total,
                            symbols_total: result.symbols_total,
                            relationships_total: result.relationships_total,
                            embed_count,
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
    ) -> Result<CallToolResult> {
        info!("Refreshing workspace: {}", workspace_id);

        // Refuse all refresh work while a primary workspace swap is mid-flight.
        // Rationale (Findings #28/#29): both the force=true primary reindex branch
        // and the post-refresh `initialize_workspace_with_force` rebind below
        // mutate the same session state the swap machinery guards. Note that
        // `current_workspace_id()` returns `None` during a swap, so we can't
        // condition this check on "targets current primary" — the safe move is
        // to back off entirely for the brief window the swap holds the flag.
        if handler.is_primary_workspace_swap_in_progress() {
            return Err(anyhow::anyhow!(
                "Primary workspace swap in progress; retry 'refresh' after the swap completes."
            ));
        }

        if self.force.unwrap_or(false)
            && handler.current_workspace_id().as_deref() == Some(workspace_id)
        {
            return self
                .handle_index_command(handler, None, self.force.unwrap_or(false), false)
                .await;
        }

        match self
            .refresh_workspace_internal(handler, workspace_id)
            .await?
        {
            RefreshWorkspaceOutcome::Success(success) => {
                if handler.current_workspace_id().as_deref() == Some(workspace_id)
                    && handler.loaded_workspace_id().as_deref() != Some(workspace_id)
                {
                    // Defensive re-check: a swap could theoretically start between
                    // refresh_workspace_internal returning and this rebind call.
                    if handler.is_primary_workspace_swap_in_progress() {
                        return Err(anyhow::anyhow!(
                            "Primary workspace swap in progress; retry 'refresh' after the swap completes."
                        ));
                    }
                    handler
                        .initialize_workspace_with_force(
                            Some(success.workspace_path.clone()),
                            false,
                        )
                        .await?;
                }

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
                if success.embed_count > 0 {
                    message.push_str(&format!(
                        "\nEmbedding {} symbols in background...",
                        success.embed_count
                    ));
                }
                Ok(CallToolResult::text_content(vec![Content::text(message)]))
            }
            RefreshWorkspaceOutcome::Failure(message) => {
                Ok(CallToolResult::text_content(vec![Content::text(message)]))
            }
        }
    }

    /// Handle stats command - show workspace statistics
    pub(crate) async fn handle_stats_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: Option<String>,
    ) -> Result<CallToolResult> {
        info!("Showing workspace statistics");

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            match workspace_id {
                Some(ref id) => match db.get_workspace(id) {
                    Ok(Some(ws)) => {
                        let message = format!(
                            "Workspace Statistics: {}\n\n\
                                {} ({})\n\
                                Path: {}\n\
                                Status: {}\n\
                                Files: {} | Symbols: {}\n\
                                Sessions: {}\n\
                                Last Indexed: {}\n\
                                Vector Count: {}",
                            ws.workspace_id,
                            ws.workspace_id
                                .split('_')
                                .next()
                                .unwrap_or(&ws.workspace_id),
                            ws.workspace_id,
                            ws.path,
                            ws.status,
                            ws.file_count.unwrap_or(0),
                            ws.symbol_count.unwrap_or(0),
                            ws.session_count,
                            ws.last_indexed
                                .map(|t| t.to_string())
                                .unwrap_or_else(|| "never".to_string()),
                            ws.vector_count.unwrap_or(0),
                        );
                        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                    }
                    Ok(None) => {
                        let message = format!("Workspace not found: {}", id);
                        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                    }
                    Err(e) => {
                        let message = format!("Failed to look up workspace: {}", e);
                        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                    }
                },
                None => {
                    let primary_workspace_id = handler.require_primary_workspace_identity()?;
                    let all_workspaces = match db.list_workspaces() {
                        Ok(workspaces) => workspaces,
                        Err(e) => {
                            let message = format!("Failed to list workspaces: {}", e);
                            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                        }
                    };
                    let pair_count = match db.list_references(&primary_workspace_id) {
                        Ok(references) => references.len(),
                        Err(e) => {
                            let message = format!("Failed to list workspace pairings: {}", e);
                            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                        }
                    };

                    let total_files: i64 = all_workspaces
                        .iter()
                        .map(|r| r.file_count.unwrap_or(0))
                        .sum();
                    let total_symbols: i64 = all_workspaces
                        .iter()
                        .map(|r| r.symbol_count.unwrap_or(0))
                        .sum();

                    let message = format!(
                        "Overall Workspace Statistics\n\n\
                        Registry Status\n\
                        Current Workspace: {}\n\
                        Known Workspaces: {}\n\
                        Current Workspace Pairings: {}\n\n\
                        Storage Usage\n\
                        Total Files: {}\n\
                        Total Symbols: {}",
                        primary_workspace_id,
                        all_workspaces.len(),
                        pair_count,
                        total_files,
                        total_symbols,
                    );
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            }
        }

        // Stdio mode: workspace statistics require daemon mode
        let message = "No workspace statistics available. Start the daemon with `julie daemon`.";
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }
}
