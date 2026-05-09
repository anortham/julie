//! `blast_radius` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::BlastRadiusTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_blast_radius, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "blast_radius",
        description = "Deterministic impact analysis for changed symbols, files, or revision ranges. Returns impacts ranked by centrality and hops, likely tests, deleted files, and a spillover handle for long lists. **Use before refactoring or after a change** to see affected callers and tests.",
        annotations(
            title = "Blast Radius",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn blast_radius(
        &self,
        Parameters(params): Parameters<BlastRadiusTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("💥 Blast radius: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = tool_targets::blast_radius_metadata(&params);
        let source_file_paths = params.file_paths.clone();
        let result = match params.call_tool(self).await {
            Ok(result) => result,
            Err(e) => {
                let message = format!("blast_radius failed: {}", e);
                self.record_tool_failure(
                    "blast_radius",
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
            "blast_radius",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
