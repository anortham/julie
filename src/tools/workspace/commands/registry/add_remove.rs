use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::workspace::registry::generate_workspace_id;
use anyhow::Result;
use tracing::{debug, info, warn};

impl ManageWorkspaceTool {
    /// Handle add command — register a reference workspace and index it.
    pub(crate) async fn handle_add_command(
        &self,
        handler: &JulieServerHandler,
        path: &str,
        name: Option<String>,
    ) -> Result<CallToolResult> {
        info!("Adding reference workspace: {}", path);

        // Daemon mode: use DaemonDatabase for registry operations
        if let Some(ref db) = handler.daemon_db {
            let primary_workspace_id = handler.workspace_id.as_deref().unwrap_or("primary");
            let workspace_path = std::path::PathBuf::from(path);
            let path_str = workspace_path
                .canonicalize()
                .unwrap_or_else(|_| workspace_path.clone())
                .to_string_lossy()
                .to_string();

            let ref_workspace_id = match generate_workspace_id(&path_str) {
                Ok(id) => id,
                Err(e) => {
                    let message = format!("Failed to generate workspace ID for {}: {}", path, e);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            };

            let dir_name = workspace_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&ref_workspace_id);
            let display_name = name.unwrap_or_else(|| dir_name.to_string());

            // Instant attach: if already indexed, just record the reference relationship
            if let Ok(Some(existing)) = db.get_workspace(&ref_workspace_id) {
                if existing.status == "ready" {
                    debug!(
                        "Reference workspace {} already indexed, instant attach",
                        ref_workspace_id
                    );
                    if let Err(e) = db.add_reference(primary_workspace_id, &ref_workspace_id) {
                        warn!("Failed to record reference relationship: {}", e);
                    }
                    let message = format!(
                        "Reference workspace attached (already indexed)!\n\
                         Workspace ID: {}\n\
                         Display Name: {}\n\
                         Path: {}\n\
                         Files: {} | Symbols: {}",
                        ref_workspace_id,
                        display_name,
                        existing.path,
                        existing.file_count.unwrap_or(0),
                        existing.symbol_count.unwrap_or(0),
                    );
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            }

            // Register as indexing before we start
            if let Err(e) = db.upsert_workspace(&ref_workspace_id, &path_str, "indexing") {
                warn!("Failed to register reference workspace in daemon.db: {}", e);
            }

            info!("Starting indexing of reference workspace: {}", display_name);

            match self
                .index_workspace_files(handler, &workspace_path, false)
                .await
            {
                Ok(result) => {
                    // Mark as ready and record stats
                    if let Err(e) = db.update_workspace_status(&ref_workspace_id, "ready") {
                        warn!("Failed to update reference workspace status: {}", e);
                    }
                    if let Err(e) = db.update_workspace_stats(
                        &ref_workspace_id,
                        result.symbols_total as i64,
                        result.files_total as i64,
                        None,
                        None,
                        Some(result.duration_ms),
                    ) {
                        warn!("Failed to update reference workspace stats: {}", e);
                    }
                    if let Err(e) = db.add_reference(primary_workspace_id, &ref_workspace_id) {
                        warn!("Failed to record reference relationship: {}", e);
                    }

                    let embed_count =
                        crate::tools::workspace::indexing::embeddings::spawn_reference_embedding(
                            handler,
                            ref_workspace_id.clone(),
                        )
                        .await;

                    let mut message = format!(
                        "Reference workspace added and indexed!\n\
                         Workspace ID: {}\n\
                         Display Name: {}\n\
                         Path: {}\n\
                         {} files, {} symbols, {} relationships",
                        ref_workspace_id,
                        display_name,
                        path_str,
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
                    if let Err(ue) = db.update_workspace_status(&ref_workspace_id, "error") {
                        warn!("Failed to update workspace status to error: {}", ue);
                    }
                    let message = format!(
                        "Reference workspace registered but indexing failed!\n\
                         Workspace ID: {}\n\
                         Display Name: {}\n\
                         Path: {}\n\
                         Error: {}",
                        ref_workspace_id, display_name, path_str, e,
                    );
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            }
        }

        // Stdio mode: workspace registry requires daemon mode
        let message =
            "Reference workspaces require daemon mode. Start the daemon with `julie daemon`.";
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }

    /// Handle remove command - remove workspace by ID and clean up index data.
    pub(crate) async fn handle_remove_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<CallToolResult> {
        info!("Removing workspace: {}", workspace_id);

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            let primary_workspace_id = handler.workspace_id.as_deref().unwrap_or("primary");

            match db.get_workspace(workspace_id) {
                Ok(Some(ws_row)) => {
                    // Delete index directory. In daemon mode, ref workspace indexes live
                    // under the primary workspace's index root (workspace_index_path).
                    if let Ok(Some(primary_ws)) = handler.get_workspace().await {
                        // indexes_root_path() in daemon mode = ~/.julie/indexes/{primary_id}
                        // Ref workspace index = indexes_root_path()/{ref_id}/db
                        let workspace_index_path =
                            primary_ws.indexes_root_path().join(workspace_id).join("db");
                        let index_dir = workspace_index_path
                            .parent()
                            .unwrap_or(&workspace_index_path);
                        if index_dir.exists() {
                            match tokio::fs::remove_dir_all(index_dir).await {
                                Ok(()) => {
                                    info!("Deleted workspace index for {}", workspace_id);
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to delete workspace directory for {}: {}",
                                        workspace_id, e
                                    );
                                }
                            }
                        }
                    }

                    // Remove reference relationship
                    if let Err(e) = db.remove_reference(primary_workspace_id, workspace_id) {
                        warn!("Failed to remove reference relationship: {}", e);
                    }

                    // Remove from daemon.db
                    if let Err(e) = db.delete_workspace(workspace_id) {
                        let message = format!("Failed to remove workspace from daemon.db: {}", e);
                        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                    }

                    let message = format!(
                        "Workspace Removed Successfully\n\
                        Workspace: {}\n\
                        Path: {}\n\
                        All associated index data removed.",
                        workspace_id, ws_row.path,
                    );
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
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

        // Stdio mode: workspace registry requires daemon mode
        let message =
            "Workspace removal requires daemon mode. Start the daemon with `julie daemon`.";
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }
}
