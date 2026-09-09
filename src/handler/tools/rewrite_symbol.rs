//! `rewrite_symbol` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::mcp_compat::{CallToolResultExt, Content};
use crate::tools::editing::ast_validation::{SyntaxValidationError, validate_no_syntax_regression};
use crate::tools::editing::syntax::{SyntaxAdapter, SyntaxConfig};
use crate::tools::metrics::session::ToolCallReport;
use crate::workspace_runtime::{PreparedSourceChange, SourceEditCoordinator, SourceEditError};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

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
        self.execute_rewrite_symbol(params)
            .await
            .map_err(|e| classify_tool_failure("rewrite_symbol", &e))
    }

    pub(crate) async fn execute_rewrite_symbol(
        &self,
        params: crate::tools::editing::rewrite_symbol::RewriteSymbolTool,
    ) -> Result<CallToolResult, anyhow::Error> {
        self.execute_rewrite_symbol_with_context(
            params,
            Instant::now() + Duration::from_secs(30),
            &CancellationToken::new(),
        )
        .await
    }

    pub(crate) async fn execute_rewrite_symbol_with_context(
        &self,
        params: crate::tools::editing::rewrite_symbol::RewriteSymbolTool,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<CallToolResult, anyhow::Error> {
        if cancellation.is_cancelled() {
            return Err(anyhow::anyhow!(
                crate::request_engine::RequestFailure::cancelled("Source edit was cancelled")
            ));
        }

        debug!(
            "✏️ rewrite_symbol: {} {} (dry_run={})",
            params.operation, params.symbol, params.dry_run
        );
        let start = Instant::now();
        let workspace_snapshot = if params.workspace.as_deref().unwrap_or("primary") == "primary" {
            self.require_primary_workspace_binding().ok()
        } else {
            None
        };

        // 1. Prepare rewrite in memory across all operations
        let prepared = match params.prepare_rewrite(self).await {
            Ok(p) => p,
            Err(e) => {
                let metadata = tool_targets::with_failure_kind(
                    tool_targets::rewrite_symbol_metadata(&params),
                    crate::tools::editing::rewrite_symbol::failure_kind(&e),
                );
                let source_file_paths = params.file_path.clone().into_iter().collect::<Vec<_>>();
                self.record_tool_failure(
                    "rewrite_symbol",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    source_file_paths,
                    Self::input_bytes_from_metadata(&metadata),
                    &format!("rewrite_symbol failed: {e}"),
                );
                return Err(e);
            }
        };

        // 2. Handle dry-run preview: 0 writes, 0 locks, 0 journals
        if params.dry_run {
            let metadata = tool_targets::merge_object(
                tool_targets::rewrite_symbol_metadata(&params),
                params.success_metrics_metadata_from_prepared(&prepared),
            );
            let result = params.call_prepared(prepared)?;
            let output_bytes = Self::output_bytes_from_result(&result);
            let rel_path_str = metadata
                .get("file_path")
                .and_then(serde_json::Value::as_str)
                .map(|p| p.to_string())
                .unwrap_or_else(|| params.file_path.clone().unwrap_or_default());
            let report = ToolCallReport {
                result_count: None,
                input_bytes: Self::input_bytes_from_metadata(&metadata),
                source_bytes: None,
                output_bytes,
                metadata,
                source_file_paths: if rel_path_str.is_empty() {
                    vec![]
                } else {
                    vec![rel_path_str]
                },
            };
            self.record_tool_call(
                "rewrite_symbol",
                start.elapsed(),
                &report,
                workspace_snapshot.as_ref(),
            );
            return Ok(result);
        }

        let rel_path_str = prepared.indexed_file_path().to_string();
        let original_content = prepared.original_content();
        let modified_content = prepared.modified_content();

        if original_content == modified_content {
            let msg = format!(
                "No changes applied to '{}' for operation '{}'",
                params.symbol, params.operation
            );
            return Ok(CallToolResult::text_content(vec![Content::text(msg)]));
        }

        let workspace_root = self.require_primary_workspace_root()?;
        let abs_path = workspace_root.join(&rel_path_str);

        // 3. Pre-Commit Whole-File AST Syntax Regression Validation
        let adapter = SyntaxAdapter::new(SyntaxConfig::default())?;
        let baseline = adapter
            .parse_source(&abs_path, original_content, Some(deadline), None)
            .map_err(|e| anyhow::anyhow!("Failed to parse source baseline: {e}"))?;

        let edit_spans = prepared.edit_spans();

        if let Err(err) = validate_no_syntax_regression(
            &abs_path,
            modified_content,
            &baseline.diagnostics,
            &edit_spans,
            &adapter,
            Some(deadline),
            None,
        ) {
            let failure = match err {
                SyntaxValidationError::SyntaxRegression { path, message, .. } => {
                    crate::request_engine::RequestFailure::new(
                        "SYNTAX_REGRESSION",
                        message.unwrap_or_else(|| "Syntax regression detected".to_string()),
                        false,
                        serde_json::json!({ "path": path.to_string_lossy() }),
                    )
                }
                other => crate::request_engine::RequestFailure::internal(other.to_string()),
            };
            let metadata = tool_targets::with_failure_kind(
                tool_targets::rewrite_symbol_metadata(&params),
                &failure.code,
            );
            self.record_tool_failure(
                "rewrite_symbol",
                start.elapsed(),
                workspace_snapshot.as_ref(),
                metadata,
                vec![rel_path_str.clone()],
                None,
                &failure.message,
            );
            return Err(anyhow::anyhow!(failure));
        }

        // 4. Atomic Coordinator Apply with Durable Journal
        let coordinator = SourceEditCoordinator::new(workspace_root)?;
        let before_hash = blake3::hash(original_content.as_bytes())
            .to_hex()
            .to_string();
        let change = PreparedSourceChange {
            path: PathBuf::from(&rel_path_str),
            before_hash,
            after_bytes: modified_content.as_bytes().to_vec(),
            before_bytes: Some(original_content.as_bytes().to_vec()),
            is_ast_aware: true,
        };
        let edit_id = uuid::Uuid::new_v4().to_string();

        match coordinator
            .apply(&edit_id, &[change], deadline, cancellation)
            .await
        {
            Ok(_disposition) => {
                let msg = format!(
                    "Applied {} on '{}' in {}:\n\n{}",
                    params.operation,
                    params.symbol,
                    rel_path_str,
                    prepared.diff()
                );
                let result = CallToolResult::text_content(vec![Content::text(msg)]);
                let metadata = tool_targets::merge_object(
                    tool_targets::rewrite_symbol_metadata(&params),
                    params.success_metrics_metadata_from_prepared(&prepared),
                );
                let report = ToolCallReport {
                    result_count: None,
                    input_bytes: Self::input_bytes_from_metadata(&metadata),
                    source_bytes: None,
                    output_bytes: Self::output_bytes_from_result(&result),
                    metadata,
                    source_file_paths: vec![rel_path_str.clone()],
                };
                self.record_tool_call(
                    "rewrite_symbol",
                    start.elapsed(),
                    &report,
                    workspace_snapshot.as_ref(),
                );
                Ok(result)
            }
            Err(e) => {
                let failure = e.to_request_failure();
                let failure_kind = match &e {
                    SourceEditError::Io(_) => "execution_error",
                    _ => &failure.code,
                };
                let metadata = tool_targets::with_failure_kind(
                    tool_targets::rewrite_symbol_metadata(&params),
                    failure_kind,
                );
                let metadata =
                    tool_targets::merge_object(metadata, serde_json::json!({ "applied": false }));
                self.record_tool_failure(
                    "rewrite_symbol",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata,
                    vec![rel_path_str],
                    None,
                    &failure.message,
                );
                Err(anyhow::anyhow!(failure))
            }
        }
    }
}
