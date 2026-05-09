//! `manage_workspace` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::info;

use crate::handler::JulieServerHandler;
use crate::tools::ManageWorkspaceTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_manage_workspace, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "manage_workspace",
        description = "Manage workspaces: index, open, register metadata, remove, list, refresh, stats, and health-check. For cross-workspace work, call open first, then pass the workspace_id to other tools.",
        annotations(
            title = "Manage Workspace",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn manage_workspace(
        &self,
        Parameters(params): Parameters<ManageWorkspaceTool>,
    ) -> Result<CallToolResult, McpError> {
        info!("🏗️ Managing workspace: {}", params.operation);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = serde_json::json!({ "operation": params.operation });
        let result = match params.call_tool(self).await {
            Ok(result) => result,
            Err(e) => {
                let message = format!("manage_workspace failed: {}", e);
                self.record_tool_failure(
                    "manage_workspace",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    params.path.clone().into_iter().collect::<Vec<_>>(),
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(McpError::internal_error(message, None));
            }
        };
        let report = ToolCallReport {
            result_count: None,
            input_bytes: Self::input_bytes_from_metadata(&metadata),
            source_bytes: None,
            output_bytes: Self::output_bytes_from_result(&result),
            metadata,
            source_file_paths: Vec::new(),
        };
        self.record_tool_call(
            "manage_workspace",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
