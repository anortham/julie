//! `fast_search` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::{JulieServerHandler, search_telemetry};
use crate::tools::FastSearchTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_fast_search, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "fast_search",
        description = "Search code and symbols using unified code-aware full-text search. Supports multi-word queries with AND/OR logic, exact symbol name matches, file-path fragments, and conceptual semantic search. Optional `backend`: omitted/default lexical returns mixed file+symbol hits and may show labeled semantic fallback candidates on identifier-like zero-hit queries when embeddings are ready; explicit `lexical` stays pure lexical; `semantic` and `hybrid` are symbol-only concept search. Use lexical for file/path queries.",
        annotations(
            title = "Fast Code Search",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn fast_search(
        &self,
        Parameters(params): Parameters<FastSearchTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("⚡ Fast search: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self
            .metrics_workspace_binding_for_workspace_param(params.workspace.as_deref())
            .await;
        let executed = match params.execute_with_trace(self).await {
            Ok(executed) => executed,
            Err(e) => {
                let metadata = search_telemetry::fast_search_metadata(&params, None);
                let message = format!("fast_search failed: {}", e);
                self.record_tool_failure(
                    "fast_search",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    Vec::new(),
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(McpError::internal_error(message, None));
            }
        };
        let metadata = search_telemetry::fast_search_metadata(&params, executed.execution.as_ref());
        let result = executed.result;
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths =
            search_telemetry::fast_search_source_paths(executed.execution.as_ref());
        let report = ToolCallReport {
            result_count: executed
                .execution
                .as_ref()
                .map(|result| result.total_results.min(u32::MAX as usize) as u32),
            input_bytes: Self::input_bytes_from_metadata(&metadata),
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "fast_search",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
