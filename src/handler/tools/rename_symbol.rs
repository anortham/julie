//! `rename_symbol` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::mcp_compat::{CallToolResultExt, Content};
use crate::tools::RenameSymbolTool;
use crate::tools::editing::ast_validation::{SyntaxValidationError, validate_no_syntax_regression};
use crate::tools::editing::syntax::{SyntaxAdapter, SyntaxConfig};
use crate::tools::metrics::session::ToolCallReport;
use crate::workspace_runtime::{PreparedSourceChange, SourceEditCoordinator, SourceEditError};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

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
        self.execute_rename_symbol(params)
            .await
            .map_err(|e| classify_tool_failure("rename_symbol", &e))
    }

    pub(crate) async fn execute_rename_symbol(
        &self,
        params: RenameSymbolTool,
    ) -> Result<CallToolResult, anyhow::Error> {
        self.execute_rename_symbol_with_context(
            params,
            Instant::now() + Duration::from_secs(30),
            &CancellationToken::new(),
        )
        .await
    }

    pub(crate) async fn execute_rename_symbol_with_context(
        &self,
        params: RenameSymbolTool,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<CallToolResult, anyhow::Error> {
        if cancellation.is_cancelled() {
            return Err(anyhow::anyhow!(
                crate::request_engine::RequestFailure::cancelled("Source edit was cancelled")
            ));
        }

        debug!("✏️ Rename symbol: {:?}", params);
        let start = Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let workspace_root = self.require_primary_workspace_root()?;

        // 1. Prepare all file modifications in memory
        let prepared = match params.prepare_rename(self).await {
            Ok(p) => p,
            Err(e) => {
                let metadata = tool_targets::with_failure_kind(
                    tool_targets::rename_symbol_metadata(&params),
                    crate::tools::refactoring::failure_kind(&e),
                );
                let message = format!("rename_symbol preparation failed: {}", e);
                self.record_tool_failure(
                    "rename_symbol",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata,
                    vec![],
                    None,
                    &message,
                );
                return Err(e);
            }
        };

        if prepared.is_empty() {
            let msg = format!(
                "rename_symbol: no references found or no changes needed for '{}'",
                params.old_name
            );
            return Ok(CallToolResult::text_content(vec![Content::text(msg)]));
        }

        // 2. Handle dry-run preview: 0 writes, 0 locks, 0 journals
        if params.dry_run {
            let result = params.format_preview(&prepared);
            let output_bytes = Self::output_bytes_from_result(&result);
            let source_file_paths = prepared.files.iter().map(|f| f.file_path.clone()).collect();
            let metadata = params.metrics_metadata_from_prepared(&prepared);
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
            return Ok(result);
        }

        // 3. Pre-Commit Whole-File AST Syntax Regression Validation
        // Validate EVERY file in the batch before taking locks or touching disk.
        let adapter = SyntaxAdapter::new(SyntaxConfig::default())?;
        for file in &prepared.files {
            if cancellation.is_cancelled() {
                return Err(anyhow::anyhow!(
                    crate::request_engine::RequestFailure::cancelled("Source edit was cancelled")
                ));
            }
            let abs_path = workspace_root.join(&file.file_path);
            let baseline = adapter
                .parse_source(&abs_path, &file.original_content, Some(deadline), None)
                .map_err(|e| anyhow::anyhow!("Failed to parse source baseline: {e}"))?;

            if let Err(err) = validate_no_syntax_regression(
                &abs_path,
                &file.modified_content,
                &baseline.diagnostics,
                &file.edit_spans,
                &adapter,
                Some(deadline),
                None,
            ) {
                let failure = match err {
                    SyntaxValidationError::SyntaxRegression { path, message, .. } => {
                        crate::request_engine::RequestFailure::new(
                            "SYNTAX_REGRESSION",
                            message.unwrap_or_else(|| {
                                "Syntax regression detected in renamed file".to_string()
                            }),
                            false,
                            serde_json::json!({ "path": path.to_string_lossy() }),
                        )
                    }
                    other => crate::request_engine::RequestFailure::internal(other.to_string()),
                };
                let metadata = tool_targets::with_failure_kind(
                    tool_targets::rename_symbol_metadata(&params),
                    &failure.code,
                );
                self.record_tool_failure(
                    "rename_symbol",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata,
                    vec![file.file_path.clone()],
                    None,
                    &failure.message,
                );
                return Err(anyhow::anyhow!(failure));
            }
        }

        // 4. Atomic Multi-File Journaled Commit
        let workspace_id = self.require_primary_workspace_identity()?;
        let store = self
            .checkout_store_for_workspace(&workspace_id, &workspace_root)
            .await?;
        let coordinator =
            SourceEditCoordinator::new(workspace_root.clone())?.with_store(store, workspace_id);
        let changes: Vec<PreparedSourceChange> = prepared
            .files
            .iter()
            .map(|f| PreparedSourceChange {
                path: PathBuf::from(&f.file_path),
                before_hash: blake3::hash(f.original_content.as_bytes())
                    .to_hex()
                    .to_string(),
                after_bytes: f.modified_content.clone().into_bytes(),
                before_bytes: Some(f.original_content.clone().into_bytes()),
                is_ast_aware: true,
            })
            .collect();

        let edit_id = uuid::Uuid::new_v4().to_string();

        match coordinator
            .apply(&edit_id, &changes, deadline, cancellation)
            .await
        {
            Ok(disposition) => {
                let file_list: Vec<String> = disposition
                    .applied_paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                let summary = format!(
                    "rename_symbol applied — applied {} change(s) to {}",
                    prepared.total_changes,
                    file_list.join(", ")
                );
                let result = CallToolResult::text_content(vec![Content::text(summary)]);
                let output_bytes = Self::output_bytes_from_result(&result);
                let metadata = params.metrics_metadata_from_prepared(&prepared);
                let report = ToolCallReport {
                    result_count: None,
                    input_bytes: Self::input_bytes_from_metadata(&metadata),
                    source_bytes: None,
                    output_bytes,
                    metadata,
                    source_file_paths: file_list,
                };
                self.record_tool_call(
                    "rename_symbol",
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
                    params.metrics_metadata_from_prepared(&prepared),
                    failure_kind,
                );
                let metadata =
                    tool_targets::merge_object(metadata, serde_json::json!({ "applied": false }));
                self.record_tool_failure(
                    "rename_symbol",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata,
                    prepared.files.iter().map(|f| f.file_path.clone()).collect(),
                    None,
                    &failure.message,
                );
                Err(anyhow::anyhow!(failure))
            }
        }
    }
}
