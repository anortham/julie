//! `rewrite_symbol` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_rewrite_symbol, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "rewrite_symbol",
        description = "Rewrite a symbol by name without reading the file first. Operations: replace_full, replace_body, replace_signature, insert_after, insert_before, add_doc. Julie resolves the symbol from the index, reparses the live file, and rewrites the live symbol span or a node-derived subspan. Always dry_run=true first to preview changes.",
        annotations(
            title = "Rewrite Symbol",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn rewrite_symbol(
        &self,
        Parameters(params): Parameters<crate::tools::editing::rewrite_symbol::RewriteSymbolTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!(
            "✏️ rewrite_symbol: {} {} (dry_run={})",
            params.operation, params.symbol, params.dry_run
        );
        let start = std::time::Instant::now();
        let workspace_snapshot = if params.workspace.as_deref().unwrap_or("primary") == "primary" {
            self.require_primary_workspace_binding().ok()
        } else {
            None
        };
        let prepared = match params.prepare_rewrite(self).await {
            Ok(prepared) => prepared,
            Err(e) => {
                let metadata = tool_targets::with_failure_kind(
                    tool_targets::rewrite_symbol_metadata(&params),
                    crate::tools::editing::rewrite_symbol::failure_kind(&e),
                );
                let source_file_paths = params.file_path.clone().into_iter().collect::<Vec<_>>();
                let message = format!("rewrite_symbol failed: {}", e);
                self.record_tool_failure(
                    "rewrite_symbol",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    source_file_paths,
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(McpError::internal_error(message, None));
            }
        };
        let metadata = tool_targets::merge_object(
            tool_targets::rewrite_symbol_metadata(&params),
            params.success_metrics_metadata_from_prepared(&prepared),
        );
        let source_file_paths = metadata
            .get("file_path")
            .and_then(serde_json::Value::as_str)
            .map(|path| vec![path.to_string()])
            .unwrap_or_else(|| params.file_path.clone().into_iter().collect::<Vec<_>>());
        let result = match params.call_prepared(prepared) {
            Ok(result) => result,
            Err(e) => {
                let metadata = tool_targets::with_failure_kind(
                    metadata.clone(),
                    crate::tools::editing::rewrite_symbol::failure_kind(&e),
                );
                let message = format!("rewrite_symbol failed: {}", e);
                self.record_tool_failure(
                    "rewrite_symbol",
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
        let source_file_paths = metadata
            .get("file_path")
            .and_then(serde_json::Value::as_str)
            .map(|path| vec![path.to_string()])
            .unwrap_or_else(|| Self::extract_paths_from_result(&result));
        let report = ToolCallReport {
            result_count: None,
            input_bytes: Self::input_bytes_from_metadata(&metadata),
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "rewrite_symbol",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
