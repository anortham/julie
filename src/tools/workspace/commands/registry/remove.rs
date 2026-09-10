use super::cleanup::{
    CLEANUP_ACTION_MANUAL_DELETE, CLEANUP_REASON_USER_REQUEST, WorkspaceDeleteOutcome,
    delete_workspace_if_allowed,
};
use super::{ManageWorkspaceTool, cleanup_activity_for_handler, registry_store_for_handler};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::info;

impl ManageWorkspaceTool {
    /// Handle remove command - remove workspace by ID and clean up index data.
    pub(crate) async fn handle_remove_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<CallToolResult> {
        info!("Removing workspace: {}", workspace_id);

        if let Some(registry_store) = registry_store_for_handler(handler)? {
            let cleanup_activity = cleanup_activity_for_handler(handler).await;
            return Ok(
                match delete_workspace_if_allowed(
                    &registry_store,
                    &cleanup_activity,
                    workspace_id,
                    CLEANUP_ACTION_MANUAL_DELETE,
                    CLEANUP_REASON_USER_REQUEST,
                )
                .await?
                {
                    WorkspaceDeleteOutcome::Deleted { workspace_id, path } => {
                        CallToolResult::text_content(vec![Content::text(format!(
                            "Workspace Removed Successfully\nWorkspace: {}\nPath: {}\nAll associated index data removed.",
                            workspace_id, path
                        ))])
                    }
                    WorkspaceDeleteOutcome::Blocked {
                        workspace_id,
                        path,
                        reason,
                    } => CallToolResult::text_content(vec![Content::text(format!(
                        "Workspace Delete Blocked\nWorkspace: {}\nPath: {}\nReason: {}",
                        workspace_id, path, reason
                    ))]),
                    WorkspaceDeleteOutcome::NotFound { workspace_id } => {
                        CallToolResult::text_content(vec![Content::text(format!(
                            "Workspace not found: {}",
                            workspace_id
                        ))])
                    }
                },
            );
        }

        // The in-process server does not wire a workspace registry.
        let message = "Workspace removal requires the workspace registry, which is not available in the in-process server.";
        Ok(CallToolResult::error(vec![Content::text(message)]))
    }
}
