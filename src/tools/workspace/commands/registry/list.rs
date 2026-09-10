use super::cleanup::run_cleanup_sweep;
use super::{ManageWorkspaceTool, cleanup_activity_for_handler, registry_store_for_handler};
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

        if let Some(registry_store) = registry_store_for_handler(handler)? {
            let cleanup_activity = cleanup_activity_for_handler(handler).await;
            let cleanup_warning = match run_cleanup_sweep(&registry_store, &cleanup_activity).await
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

            let all_workspaces = match registry_store.list_workspaces() {
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
}
