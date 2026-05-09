//! `get_symbols` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::GetSymbolsTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_get_symbols, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "get_symbols",
        description = "Get symbols (functions, classes, etc.) from a file without reading full content. Requires exact file path — use deep_dive(symbol=...) if you don't know the path.",
        annotations(
            title = "Get File Symbols",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_symbols(
        &self,
        Parameters(params): Parameters<GetSymbolsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📋 Get symbols for file: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = tool_targets::get_symbols_metadata(&params);
        let input_bytes = Self::input_bytes_from_metadata(&metadata);
        let source_file_paths = vec![params.file_path.clone()];
        let result = match params.call_tool(self).await {
            Ok(result) => result,
            Err(e) => {
                let message = format!("get_symbols failed: {}", e);
                self.record_tool_failure(
                    "get_symbols",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata,
                    source_file_paths,
                    input_bytes,
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
            source_file_paths,
        };
        self.record_tool_call(
            "get_symbols",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
