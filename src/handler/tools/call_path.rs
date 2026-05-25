//! `call_path` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_call_path, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "call_path",
        description = "Find one shortest call-graph path between two symbols. Use it for \"how does A reach B?\" questions or to trace one caller chain between two known symbols. Traverses calls, instantiations, and overrides only; use `from_file_path` / `to_file_path` when shared names are ambiguous. Returns a compact hop list when a path exists, or found=false with a short diagnostic when it does not.",
        annotations(
            title = "Call Path",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn call_path(
        &self,
        Parameters(params): Parameters<crate::tools::navigation::CallPathTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("🧭 call_path: {} -> {}", params.from, params.to);
        let start = std::time::Instant::now();
        let workspace_snapshot = if params.workspace.as_deref().unwrap_or("primary") == "primary" {
            self.require_primary_workspace_binding().ok()
        } else {
            None
        };
        let metadata = tool_targets::call_path_metadata(&params);
        let source_file_paths = [params.from_file_path.clone(), params.to_file_path.clone()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let result = match params.call_tool(self).await {
            Ok(result) => result,
            Err(e) => {
                let message = format!("call_path failed: {}", e);
                self.record_tool_failure(
                    "call_path",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    source_file_paths.clone(),
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(classify_tool_failure("call_path", &e));
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
            "call_path",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
