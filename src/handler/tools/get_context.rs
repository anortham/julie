//! `get_context` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::GetContextTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_get_context, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "get_context",
        description = "Get token-budgeted context for a concept or task. Returns a relevant code subgraph with pivots (full code) and neighbors (signatures). Use at the start of a task for orientation. Task inputs `edited_files`, `entry_symbols`, `stack_trace`, `failing_test`, `max_hops`, and `prefer_tests` focus the subgraph.",
        annotations(
            title = "Get Context",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_context(
        &self,
        Parameters(params): Parameters<GetContextTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📦 Get context: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self
            .metrics_workspace_binding_for_workspace_param(params.workspace.as_deref())
            .await;
        let metadata = tool_targets::get_context_metadata(&params);
        let source_file_paths = params.edited_files.clone().unwrap_or_default();
        let result = match params.call_tool(self).await {
            Ok(result) => result,
            Err(e) => {
                let message = format!("get_context failed: {}", e);
                self.record_tool_failure(
                    "get_context",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    source_file_paths.clone(),
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(McpError::internal_error(message, None));
            }
        };
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let report = ToolCallReport {
            result_count: None,
            input_bytes: Self::input_bytes_from_metadata(&metadata),
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "get_context",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
