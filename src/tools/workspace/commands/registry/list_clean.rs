use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::info;

impl ManageWorkspaceTool {
    /// Handle list command - show all registered workspaces.
    pub(crate) async fn handle_list_command(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        info!("Listing all workspaces");

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            let primary_workspace_id = handler.workspace_id.as_deref().unwrap_or("primary");

            let references = db.list_references(primary_workspace_id).unwrap_or_default();
            let primary_row = db.get_workspace(primary_workspace_id).ok().flatten();

            if primary_row.is_none() && references.is_empty() {
                let message = "No workspaces registered.";
                return Ok(CallToolResult::text_content(vec![Content::text(message)]));
            }

            let mut output = String::from("Registered Workspaces:\n\n");

            if let Some(pw) = primary_row {
                output.push_str(&format!(
                    "{} ({}) [PRIMARY]\n\
                    Path: {}\n\
                    Status: {} | Sessions: {}\n\
                    Files: {} | Symbols: {}\n\n",
                    pw.workspace_id
                        .split('_')
                        .next()
                        .unwrap_or(&pw.workspace_id),
                    pw.workspace_id,
                    pw.path,
                    pw.status,
                    pw.session_count,
                    pw.file_count.unwrap_or(0),
                    pw.symbol_count.unwrap_or(0),
                ));
            }

            for ws in &references {
                let path_exists = std::path::Path::new(&ws.path).exists();
                let status_str = if !path_exists { "MISSING" } else { &ws.status };
                output.push_str(&format!(
                    "{} ({}) [REFERENCE]\n\
                    Path: {}\n\
                    Status: {}\n\
                    Files: {} | Symbols: {}\n\n",
                    ws.workspace_id
                        .split('_')
                        .next()
                        .unwrap_or(&ws.workspace_id),
                    ws.workspace_id,
                    ws.path,
                    status_str,
                    ws.file_count.unwrap_or(0),
                    ws.symbol_count.unwrap_or(0),
                ));
            }

            return Ok(CallToolResult::text_content(vec![Content::text(output)]));
        }

        // Stdio mode: no workspace registry available
        let message = "No workspaces registered.";
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }

    /// Handle clean command - clean expired/orphaned workspaces
    pub(crate) async fn handle_clean_command(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        info!("Cleaning workspaces (comprehensive cleanup: TTL + Size Limits + Orphans)");

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            let all_workspaces = match db.list_workspaces() {
                Ok(ws) => ws,
                Err(e) => {
                    let message = format!("Failed to list workspaces: {}", e);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            };

            let mut cleaned = Vec::new();
            for ws in &all_workspaces {
                if !std::path::Path::new(&ws.path).exists() {
                    if let Err(e) = db.delete_workspace(&ws.workspace_id) {
                        tracing::warn!(
                            "Failed to delete workspace {} during cleanup: {}",
                            ws.workspace_id,
                            e
                        );
                    } else {
                        cleaned.push(ws.workspace_id.clone());
                    }
                }
            }

            let message = if cleaned.is_empty() {
                "No cleanup needed. All workspaces are healthy!".to_string()
            } else {
                format!(
                    "Cleaned up {} workspace(s) with missing paths:\n{}",
                    cleaned.len(),
                    cleaned.join("\n"),
                )
            };
            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
        }

        // Stdio mode: no workspace registry available
        let message = "No cleanup needed. All workspaces are healthy!";
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }
}
