//! `spillover_get` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::SpilloverGetTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_spillover_get, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "spillover_get",
        description = "Fetch the next page for a large `get_context` or `blast_radius` result using the returned `spillover_handle`, without rerunning the underlying query.",
        annotations(
            title = "Get Spillover Page",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn spillover_get(
        &self,
        Parameters(params): Parameters<SpilloverGetTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📄 Spillover get: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = tool_targets::spillover_get_metadata(&params);
        let result = match params.call_tool(self).await {
            Ok(result) => result,
            Err(e) => {
                let message = format!("spillover_get failed: {}", e);
                self.record_tool_failure(
                    "spillover_get",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    Vec::new(),
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(classify_tool_failure("spillover_get", &e));
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
            "spillover_get",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
