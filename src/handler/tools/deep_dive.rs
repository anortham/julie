//! `deep_dive` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::DeepDiveTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_deep_dive, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "deep_dive",
        description = "Investigate a symbol with progressive depth. Returns definition, references, children, and type info in a single call — tailored to the symbol's kind.\n\n**Always use BEFORE modifying or extending a symbol.** Replaces the common chain of fast_search → get_symbols → fast_refs → Read with a single call.",
        annotations(
            title = "Deep Dive Symbol Investigation",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn deep_dive(
        &self,
        Parameters(params): Parameters<DeepDiveTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("🔍 Deep dive: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = tool_targets::deep_dive_metadata(&params);
        let result = match params.call_tool(self).await {
            Ok(result) => result,
            Err(e) => {
                let full_message = format!("deep_dive failed: {}", e);
                self.record_tool_failure(
                    "deep_dive",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    params.context_file.clone().into_iter().collect::<Vec<_>>(),
                    Self::input_bytes_from_metadata(&metadata),
                    &full_message,
                );
                return Err(classify_tool_failure("deep_dive", &e));
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
            "deep_dive",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
