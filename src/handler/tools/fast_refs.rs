//! `fast_refs` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::workspace_resolution::resolve_workspace_filter;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::FastRefsTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_fast_refs, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "fast_refs",
        description = "Find all references to a symbol across the codebase. Required before modifying any symbol. Use `reference_kind` to filter (calls, type-uses, etc.). For a broader view including definition and callers in one call, use deep_dive instead.",
        annotations(
            title = "Find References",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn fast_refs(
        &self,
        Parameters(params): Parameters<FastRefsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("⚡ Fast find references: {:?}", params);
        let start = std::time::Instant::now();
        let metadata = tool_targets::fast_refs_metadata(&params);

        // Resolve workspace ONCE per request. Used for both metrics attribution
        // and the actual tool call below, so bad workspace_id surfaces as
        // invalid_params before any other work happens.
        let workspace_target =
            match resolve_workspace_filter(params.workspace.as_deref(), self).await {
                Ok(target) => target,
                Err(e) => {
                    let message = format!("fast_refs failed: {}", e);
                    self.record_tool_failure(
                        "fast_refs",
                        start.elapsed(),
                        None,
                        metadata.clone(),
                        Vec::new(),
                        Self::input_bytes_from_metadata(&metadata),
                        &message,
                    );
                    return Err(classify_tool_failure("fast_refs", &e));
                }
            };

        let workspace_snapshot = self
            .metrics_workspace_binding_for_target(&workspace_target)
            .await;
        let result = match params.call_tool_with_target(self, &workspace_target).await {
            Ok(result) => result,
            Err(e) => {
                let message = format!("fast_refs failed: {}", e);
                self.record_tool_failure(
                    "fast_refs",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    Vec::new(),
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(classify_tool_failure("fast_refs", &e));
            }
        };
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = Self::extract_paths_from_result(&result);
        let source_bytes = self
            .metrics_source_bytes_for_binding(workspace_snapshot.as_ref(), &source_file_paths)
            .await;
        let report = ToolCallReport {
            result_count: None,
            input_bytes: Self::input_bytes_from_metadata(&metadata),
            source_bytes,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "fast_refs",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
