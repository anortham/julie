//! `edit_file` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::tools::metrics::session::ToolCallReport;

#[tool_router(router = tool_router_edit_file, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "edit_file",
        description = "Edit a file without reading it first. Provide old_text (fuzzy-matched via diff-match-patch) and new_text. Saves the full Read step that the built-in Edit tool requires. Use occurrence to control which match: \"first\" (default), \"last\", or \"all\". Always dry_run=true first to preview, then dry_run=false to apply.",
        annotations(
            title = "Edit File",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn edit_file(
        &self,
        Parameters(params): Parameters<crate::tools::editing::edit_file::EditFileTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!(
            "✏️ edit_file: {} (dry_run={})",
            params.file_path, params.dry_run
        );
        // T7 (Risk #2): refuse writes on in-process followers.
        if self.is_in_process_follower() {
            let e = anyhow::anyhow!(
                "another session owns writes for this workspace; this is a read-only follower"
            );
            return Err(classify_tool_failure("edit_file", &e));
        }
        let start = std::time::Instant::now();
        let workspace_snapshot = if params.workspace.as_deref().unwrap_or("primary") == "primary" {
            self.require_primary_workspace_binding().ok()
        } else {
            None
        };
        let prepared = match params.prepare_edit(self).await {
            Ok(prepared) => prepared,
            Err(e) => {
                let metadata = tool_targets::with_failure_kind(
                    tool_targets::edit_file_metadata(&params),
                    crate::tools::editing::edit_file::failure_kind(&e),
                );
                let message = format!("edit_file failed: {}", e);
                self.record_tool_failure(
                    "edit_file",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata,
                    vec![params.file_path.clone()],
                    Some(params.request_input_bytes()),
                    &message,
                );
                return Err(classify_tool_failure("edit_file", &e));
            }
        };
        let metadata = tool_targets::merge_object(
            params.success_metrics_metadata_from_prepared(&prepared),
            serde_json::json!({
                "file": params.file_path.clone(),
                "target": {
                    "target_symbol_name": serde_json::Value::Null,
                    "target_file_path": params.file_path.clone(),
                    "target_line": serde_json::Value::Null,
                }
            }),
        );
        let input_bytes = Self::input_bytes_from_metadata(&metadata);
        let result = match params.call_prepared(prepared) {
            Ok(result) => result,
            Err(e) => {
                let metadata = tool_targets::with_failure_kind(
                    metadata,
                    crate::tools::editing::edit_file::failure_kind(&e),
                );
                let metadata =
                    tool_targets::merge_object(metadata, serde_json::json!({ "applied": false }));
                let message = format!("edit_file failed: {}", e);
                self.record_tool_failure(
                    "edit_file",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata,
                    vec![params.file_path.clone()],
                    input_bytes,
                    &message,
                );
                return Err(classify_tool_failure("edit_file", &e));
            }
        };
        let output_bytes = Self::output_bytes_from_result(&result);
        let source_file_paths = vec![params.file_path.clone()];
        let report = ToolCallReport {
            result_count: None,
            input_bytes: Self::input_bytes_from_metadata(&metadata),
            source_bytes: None,
            output_bytes,
            metadata,
            source_file_paths,
        };
        self.record_tool_call(
            "edit_file",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
