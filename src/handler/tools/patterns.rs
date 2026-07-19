use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::workspace_resolution::resolve_workspace_filter;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::PatternsTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_patterns, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "patterns",
        description = "Query generic code-shape facts extracted across all supported languages",
        annotations(
            title = "Query Structural Patterns",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn patterns(
        &self,
        Parameters(params): Parameters<PatternsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("Query patterns: {:?}", params);
        let start = std::time::Instant::now();
        let metadata = tool_targets::patterns_metadata(&params);
        let workspace_target =
            match resolve_workspace_filter(params.workspace.as_deref(), self).await {
                Ok(target) => target,
                Err(error) => {
                    let message = format!("patterns failed: {error}");
                    self.record_tool_failure(
                        "patterns",
                        start.elapsed(),
                        None,
                        metadata.clone(),
                        Vec::new(),
                        Self::input_bytes_from_metadata(&metadata),
                        &message,
                    );
                    return Err(classify_tool_failure("patterns", &error));
                }
            };
        let workspace_snapshot = self
            .metrics_workspace_binding_for_target(&workspace_target)
            .await;
        let result = match params.call_tool_with_target(self, &workspace_target).await {
            Ok(result) => result,
            Err(error) => {
                let message = format!("patterns failed: {error}");
                self.record_tool_failure(
                    "patterns",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    Vec::new(),
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(classify_tool_failure("patterns", &error));
            }
        };
        let report = ToolCallReport {
            result_count: None,
            input_bytes: Self::input_bytes_from_metadata(&metadata),
            source_bytes: None,
            output_bytes: Self::output_bytes_from_result(&result),
            metadata,
            source_file_paths: Self::extract_paths_from_result(&result),
        };
        self.record_tool_call(
            "patterns",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
