//! `edit_file` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::mcp_compat::{CallToolResultExt, Content};
use crate::tools::metrics::session::ToolCallReport;
use crate::workspace_runtime::SourceEditError;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

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
        self.execute_edit_file(params)
            .await
            .map_err(|e| classify_tool_failure("edit_file", &e))
    }

    pub(crate) async fn execute_edit_file(
        &self,
        params: crate::tools::editing::edit_file::EditFileTool,
    ) -> Result<CallToolResult, anyhow::Error> {
        self.execute_edit_file_with_context(
            params,
            Instant::now() + Duration::from_secs(30),
            &CancellationToken::new(),
        )
        .await
    }

    pub(crate) async fn execute_edit_file_with_context(
        &self,
        params: crate::tools::editing::edit_file::EditFileTool,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<CallToolResult, anyhow::Error> {
        if cancellation.is_cancelled() {
            return Err(anyhow::anyhow!(
                crate::request_engine::RequestFailure::cancelled("Source edit was cancelled")
            ));
        }

        debug!(
            "✏️ edit_file: {} (dry_run={})",
            params.file_path, params.dry_run
        );
        let start = Instant::now();
        let workspace_snapshot = if params.workspace.as_deref().unwrap_or("primary") == "primary" {
            self.require_primary_workspace_binding().ok()
        } else {
            None
        };

        if params.dry_run {
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
                    return Err(e);
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
                    let metadata = tool_targets::merge_object(
                        metadata,
                        serde_json::json!({ "applied": false }),
                    );
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
                    return Err(e);
                }
            };
            let output_bytes = Self::output_bytes_from_result(&result);
            let report = ToolCallReport {
                result_count: None,
                input_bytes: Self::input_bytes_from_metadata(&metadata),
                source_bytes: None,
                output_bytes,
                metadata,
                source_file_paths: vec![params.file_path.clone()],
            };
            self.record_tool_call(
                "edit_file",
                start.elapsed(),
                &report,
                workspace_snapshot.as_ref(),
            );
            return Ok(result);
        }

        let root = self.require_primary_workspace_root()?;
        let resolved_path =
            julie_core::file_utils::secure_path_resolution(&params.file_path, &root)?;
        let workspace_id = self.require_primary_workspace_identity()?;
        let store = self
            .checkout_store_for_workspace(&workspace_id, &root)
            .await?;
        let coordinator = crate::workspace_runtime::SourceEditCoordinator::new(root)?
            .with_store(store, workspace_id);
        let original_bytes =
            match coordinator.read_source_bounded(&resolved_path, deadline, cancellation) {
                Ok(b) => b,
                Err(e) => {
                    let failure = e.to_request_failure();
                    let metadata = tool_targets::with_failure_kind(
                        tool_targets::edit_file_metadata(&params),
                        &failure.code,
                    );
                    let message = format!("edit_file failed: {}", failure.message);
                    self.record_tool_failure(
                        "edit_file",
                        start.elapsed(),
                        workspace_snapshot.as_ref(),
                        metadata,
                        vec![params.file_path.clone()],
                        Some(params.request_input_bytes()),
                        &message,
                    );
                    return Err(anyhow::anyhow!(failure));
                }
            };
        let original_content = match String::from_utf8(original_bytes) {
            Ok(s) => s,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "File '{}' is not valid UTF-8: {}",
                    params.file_path,
                    e
                ));
            }
        };
        let before_hash = blake3::hash(original_content.as_bytes())
            .to_hex()
            .to_string();
        let metadata = tool_targets::merge_object(
            params.base_metrics_metadata(),
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

        let modified_content = match crate::tools::editing::edit_file::apply_edit(
            &original_content,
            &params.old_text,
            &params.new_text,
            params.occurrence.as_str(),
        ) {
            Ok(content) => content,
            Err(e) => {
                let failure = crate::request_engine::RequestFailure::new(
                    "EDIT_CONFLICT",
                    format!(
                        "Target file '{}' does not match old_text: {}",
                        params.file_path, e
                    ),
                    false,
                    serde_json::json!({
                        "file_path": params.file_path,
                        "conflict_kind": "match_not_found",
                    }),
                );
                let metadata = tool_targets::with_failure_kind(metadata, &failure.code);
                let metadata =
                    tool_targets::merge_object(metadata, serde_json::json!({ "applied": false }));
                let message = format!("edit_file failed: {}", failure.message);
                self.record_tool_failure(
                    "edit_file",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata,
                    vec![params.file_path.clone()],
                    input_bytes,
                    &message,
                );
                return Err(anyhow::anyhow!(failure));
            }
        };
        let diff = crate::tools::editing::validation::format_unified_diff(
            &original_content,
            &modified_content,
            &params.file_path,
        );
        let balance_warning =
            if crate::tools::editing::validation::should_check_balance(&params.file_path) {
                crate::tools::editing::validation::check_bracket_balance(
                    &original_content,
                    &modified_content,
                )
            } else {
                None
            };
        let file_size_bytes = original_content.len();
        let diff_bytes = diff.len();
        let changed_bytes = crate::tools::editing::edit_file::changed_region_bytes(
            &original_content,
            &modified_content,
        );
        let match_mode = if original_content.contains(&params.old_text) {
            "exact"
        } else {
            "trimmed_lines"
        };
        let is_applied = !params.dry_run && modified_content != original_content;

        let change = crate::workspace_runtime::PreparedSourceChange {
            path: resolved_path,
            before_hash,
            after_bytes: modified_content.into_bytes(),
            before_bytes: Some(original_content.into_bytes()),
            is_ast_aware: false,
        };
        let edit_id = uuid::Uuid::new_v4().to_string();

        let result = match coordinator
            .apply(&edit_id, &[change], deadline, cancellation)
            .await
        {
            Ok(_disposition) => {
                let mut msg = format!("Applied edit to {}:\n\n{}", params.file_path, diff);
                if let Some(ref warning) = balance_warning {
                    msg.push_str(&format!("\n\n{}", warning));
                }
                CallToolResult::text_content(vec![Content::text(msg)])
            }
            Err(e) => {
                let failure = e.to_request_failure();
                let failure_kind = match &e {
                    SourceEditError::Io(_) => "execution_error",
                    _ => &failure.code,
                };
                let metadata = tool_targets::with_failure_kind(metadata, failure_kind);
                let metadata =
                    tool_targets::merge_object(metadata, serde_json::json!({ "applied": false }));
                let message = format!("edit_file failed: {}", failure.message);
                self.record_tool_failure(
                    "edit_file",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata,
                    vec![params.file_path.clone()],
                    input_bytes,
                    &message,
                );
                return Err(anyhow::anyhow!(failure));
            }
        };

        let metadata = tool_targets::merge_object(
            metadata,
            serde_json::json!({
                "file_size_bytes": file_size_bytes,
                "diff_bytes": diff_bytes,
                "changed_bytes": changed_bytes,
                "match_mode": match_mode,
                "applied": is_applied,
            }),
        );

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
