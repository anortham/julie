use super::ManageWorkspaceTool;
use super::cleanup::run_cleanup_sweep;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::{info, warn};

impl ManageWorkspaceTool {
    /// Handle list command - show all registered workspaces.
    pub(crate) async fn handle_list_command(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        info!("Listing all workspaces");

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            let cleanup_warning = match run_cleanup_sweep(
                db,
                handler.workspace_pool.as_ref(),
                handler.watcher_pool.as_ref(),
            )
            .await
            {
                Ok(_) => None,
                Err(error) => {
                    warn!("Workspace cleanup sweep failed during list: {}", error);
                    Some(format!("Cleanup sweep failed: {}", error))
                }
            };

            // list is orthogonal to having a primary bound. CURRENT labels
            // require a primary; everything else is either ACTIVE or KNOWN.
            let primary_workspace_id = handler.current_workspace_id();
            let active_workspace_ids: std::collections::HashSet<String> =
                handler.active_workspace_ids().await.into_iter().collect();

            let all_workspaces = match db.list_workspaces() {
                Ok(workspaces) => workspaces,
                Err(e) => {
                    let message = format!("Failed to list workspaces: {}", e);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            };

            if all_workspaces.is_empty() {
                let message = "No workspaces registered.";
                return Ok(CallToolResult::text_content(vec![Content::text(message)]));
            }

            let mut output = String::from("Registered Workspaces:\n\n");
            if let Some(warning) = cleanup_warning {
                output.push_str(&format!("Cleanup Warning: {}\n\n", warning));
            }

            for ws in &all_workspaces {
                let path_exists = std::path::Path::new(&ws.path).exists();
                let status_str = if !path_exists { "MISSING" } else { &ws.status };
                let mut labels = Vec::new();
                if Some(ws.workspace_id.as_str()) == primary_workspace_id.as_deref() {
                    labels.push("CURRENT");
                } else if active_workspace_ids.contains(&ws.workspace_id) {
                    labels.push("ACTIVE");
                } else {
                    labels.push("KNOWN");
                }
                output.push_str(&format!(
                    "{} ({}) [{}]\n\
                     Path: {}\n\
                    Status: {} | Sessions: {}\n\
                     Files: {} | Symbols: {}\n\n",
                    ws.workspace_id
                        .split('_')
                        .next()
                        .unwrap_or(&ws.workspace_id),
                    ws.workspace_id,
                    labels.join(", "),
                    ws.path,
                    status_str,
                    ws.session_count,
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
        info!("Cleaning workspaces");

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            let summary = match run_cleanup_sweep(
                db,
                handler.workspace_pool.as_ref(),
                handler.watcher_pool.as_ref(),
            )
            .await
            {
                Ok(summary) => summary,
                Err(e) => {
                    let message = format!("Failed to run workspace cleanup: {}", e);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            };

            let message = if summary.pruned_workspaces.is_empty()
                && summary.pruned_orphan_dirs.is_empty()
                && summary.blocked_workspaces.is_empty()
            {
                "No cleanup needed. All workspaces are healthy!".to_string()
            } else {
                let mut parts = Vec::new();
                if !summary.pruned_workspaces.is_empty() {
                    parts.push(format!(
                        "Removed {} stale workspaces (missing paths):\n  {}",
                        summary.pruned_workspaces.len(),
                        summary.pruned_workspaces.join("\n  "),
                    ));
                }
                if !summary.pruned_orphan_dirs.is_empty() {
                    parts.push(format!(
                        "Removed {} orphan index directories (not in DB):\n  {}",
                        summary.pruned_orphan_dirs.len(),
                        summary.pruned_orphan_dirs.join("\n  "),
                    ));
                }
                if !summary.blocked_workspaces.is_empty() {
                    let blocked = summary
                        .blocked_workspaces
                        .iter()
                        .map(|(workspace_id, reason)| format!("{workspace_id}: {reason}"))
                        .collect::<Vec<_>>()
                        .join("\n  ");
                    parts.push(format!(
                        "Skipped {} workspaces during cleanup:\n  {}",
                        summary.blocked_workspaces.len(),
                        blocked,
                    ));
                }
                parts.join("\n\n")
            };
            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
        }

        // Stdio mode: no workspace registry available
        let message = "No cleanup needed. All workspaces are healthy!";
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }
}
