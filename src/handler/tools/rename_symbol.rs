//! `rename_symbol` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::RenameSymbolTool;
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_rename_symbol, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "rename_symbol",
        description = "Rename a symbol across the entire codebase with index-aware, workspace-wide updates. Always preview with `dry_run=true` first.",
        annotations(
            title = "Rename Symbol",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn rename_symbol(
        &self,
        Parameters(params): Parameters<RenameSymbolTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("✏️ Rename symbol: {:?}", params);
        // T7 (Risk #2): refuse writes on in-process followers.
        if self.is_in_process_follower() {
            let e = anyhow::anyhow!(
                "another session owns writes for this workspace; this is a read-only follower"
            );
            return Err(classify_tool_failure("rename_symbol", &e));
        }
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = params
            .metrics_metadata(self)
            .await
            .unwrap_or_else(|_| tool_targets::rename_symbol_metadata(&params));
        let source_file_paths = params.scope.clone().into_iter().collect::<Vec<_>>();
        let result = match params.call_tool(self).await {
            Ok(result) => result,
            Err(e) => {
                let metadata = tool_targets::with_failure_kind(
                    metadata.clone(),
                    crate::tools::refactoring::failure_kind(&e),
                );
                let message = format!("rename_symbol failed: {}", e);
                self.record_tool_failure(
                    "rename_symbol",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    source_file_paths.clone(),
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(classify_tool_failure("rename_symbol", &e));
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
            "rename_symbol",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
