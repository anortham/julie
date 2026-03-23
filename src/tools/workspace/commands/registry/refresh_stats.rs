use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::{info, warn};

impl ManageWorkspaceTool {
    /// Handle refresh command - re-index workspace
    pub(crate) async fn handle_refresh_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<CallToolResult> {
        info!("Refreshing workspace: {}", workspace_id);

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            match db.get_workspace(workspace_id) {
                Ok(Some(ws_row)) => {
                    let workspace_path = std::path::PathBuf::from(&ws_row.path);
                    info!("Starting re-indexing of workspace: {}", workspace_id);

                    let force = self.force.unwrap_or(false);
                    match self
                        .index_workspace_files(handler, &workspace_path, force)
                        .await
                    {
                        Ok(result) => {
                            if let Err(e) = db.update_workspace_stats(
                                workspace_id,
                                result.symbols_total as i64,
                                result.files_total as i64,
                                None,
                                None,
                            ) {
                                warn!("Failed to update workspace stats: {}", e);
                            }

                            // Force refresh: abort running pipeline and clear embeddings
                            if force {
                                let mut task_guard = handler.embedding_task.lock().await;
                                if let Some((cancel_flag, handle)) = task_guard.take() {
                                    info!(
                                        "Cancelling running embedding pipeline for force refresh"
                                    );
                                    cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                                    handle.abort();
                                }
                            }

                            let embed_count = crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                                handler,
                                workspace_id.to_string(),
                            ).await;

                            let status = if result.files_processed == 0 {
                                "Already up-to-date.".to_string()
                            } else if force {
                                format!(
                                    "Full re-index: {} files processed.",
                                    result.files_processed
                                )
                            } else {
                                format!("{} changed files re-indexed.", result.files_processed)
                            };

                            let mut message = format!(
                                "Workspace Refresh: {}\n\
                                {}\n\
                                Path: {}\n\
                                Totals: {} files, {} symbols, {} relationships",
                                workspace_id,
                                status,
                                ws_row.path,
                                result.files_total,
                                result.symbols_total,
                                result.relationships_total,
                            );
                            if embed_count > 0 {
                                message.push_str(&format!(
                                    "\nEmbedding {} symbols in background...",
                                    embed_count
                                ));
                            }
                            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
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
                            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                        }
                    }
                }
                Ok(None) => {
                    let message = format!("Workspace not found: {}", workspace_id);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
                Err(e) => {
                    let message = format!("Failed to look up workspace: {}", e);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            }
        }

        // Stdio mode: workspace refresh requires daemon mode
        let message = format!(
            "Workspace refresh requires daemon mode. Start the daemon with `julie daemon`.\n\
             (Workspace ID: {})",
            workspace_id
        );
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
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
            let primary_workspace_id = handler.workspace_id.as_deref().unwrap_or("primary");

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
                    // Show overall stats: primary + all references
                    let primary_row = db.get_workspace(primary_workspace_id).ok().flatten();
                    let references = db.list_references(primary_workspace_id).unwrap_or_default();

                    let total_files: i64 =
                        primary_row.as_ref().and_then(|r| r.file_count).unwrap_or(0)
                            + references
                                .iter()
                                .map(|r| r.file_count.unwrap_or(0))
                                .sum::<i64>();
                    let total_symbols: i64 = primary_row
                        .as_ref()
                        .and_then(|r| r.symbol_count)
                        .unwrap_or(0)
                        + references
                            .iter()
                            .map(|r| r.symbol_count.unwrap_or(0))
                            .sum::<i64>();

                    let message = format!(
                        "Overall Workspace Statistics\n\n\
                        Registry Status\n\
                        Primary Workspace: {}\n\
                        Reference Workspaces: {}\n\n\
                        Storage Usage\n\
                        Total Files: {}\n\
                        Total Symbols: {}",
                        primary_row
                            .map(|r| r.workspace_id)
                            .unwrap_or_else(|| primary_workspace_id.to_string()),
                        references.len(),
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
