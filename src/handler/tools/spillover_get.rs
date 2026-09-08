//! `spillover_get` MCP tool.

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::CallToolResult, tool,
    tool_router,
};
use tracing::debug;

use crate::handler::tools::error::classify_tool_failure;
use crate::handler::{JulieServerHandler, tool_targets};
use crate::mcp_compat::Content;
use crate::request_engine::RequestFailure;
use crate::tools::SpilloverGetTool;
use crate::tools::metrics::session::ToolCallReport;
use crate::tools::spillover::format_page;
use crate::workspace_runtime::continuation::{ContinuationBinding, is_valid_continuation_token};
use crate::workspace_runtime::continuation_store::ContinuationStore;
use julie_context::SpilloverFormat;
use julie_core::database::SymbolDatabase;
use std::path::PathBuf;

#[tool_router(router = tool_router_spillover_get, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "spillover_get",
        description = "Fetch the next page for a large `get_context` or `blast_radius` result using the returned `spillover_handle`, without rerunning the underlying query.",
        annotations(
            title = "Get Spillover Page",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn spillover_get(
        &self,
        Parameters(params): Parameters<SpilloverGetTool>,
    ) -> Result<CallToolResult, McpError> {
        self.execute_spillover_get(params)
            .await
            .map_err(|e| classify_tool_failure("spillover_get", &e))
    }

    pub(crate) async fn execute_spillover_get(
        &self,
        params: SpilloverGetTool,
    ) -> Result<CallToolResult, anyhow::Error> {
        debug!("📄 Spillover get: {:?}", params);
        let start = std::time::Instant::now();
        let workspace_snapshot = self.require_primary_workspace_binding().ok();
        let metadata = tool_targets::spillover_get_metadata(&params);

        // 1. If this is a 64-character hex continuation token or durable store lookup:
        if is_valid_continuation_token(&params.spillover_handle) {
            let binding = match self.require_primary_workspace_binding() {
                Ok(b) => b,
                Err(e) => {
                    let failure = RequestFailure::new(
                        "CONTINUATION_INVALID",
                        format!("Workspace binding error: {e}"),
                        false,
                        serde_json::json!({}),
                    );
                    return Err(anyhow::Error::new(failure));
                }
            };

            let index_root = match self.workspace_index_dir_for(&binding.workspace_id).await {
                Ok(dir) => dir,
                Err(e) => {
                    let failure = RequestFailure::new(
                        "CONTINUATION_INVALID",
                        format!("Workspace index dir error: {e}"),
                        false,
                        serde_json::json!({}),
                    );
                    return Err(anyhow::Error::new(failure));
                }
            };

            // If an explicit foreign workspace was requested, reject with CONTINUATION_INVALID
            if let Some(ref ws) = params.workspace {
                let ws_path = PathBuf::from(ws);
                let canon = ws_path.canonicalize().unwrap_or_else(|_| ws_path.clone());
                let binding_root = binding
                    .workspace_root
                    .canonicalize()
                    .unwrap_or_else(|_| binding.workspace_root.clone());
                if ws != "primary" && canon != binding_root && ws != &binding.workspace_id {
                    let failure = RequestFailure::new(
                        "CONTINUATION_INVALID",
                        format!(
                            "Workspace mismatch: requested {ws}, current is {}",
                            binding.workspace_id
                        ),
                        false,
                        serde_json::json!({}),
                    );
                    return Err(anyhow::Error::new(failure));
                }
            }

            let store = match ContinuationStore::open(&index_root) {
                Ok(s) => s,
                Err(e) => {
                    let failure = RequestFailure::new(
                        "CONTINUATION_INVALID",
                        format!("Failed to open continuation store: {e}"),
                        false,
                        serde_json::json!({}),
                    );
                    return Err(anyhow::Error::new(failure));
                }
            };

            let (generation, source_hashes) = {
                let sym_db_path = index_root.join("db").join("symbols.db");
                if let Ok(sym_db) = SymbolDatabase::new(&sym_db_path) {
                    let canonical = sym_db
                        .get_latest_canonical_revision(&binding.workspace_id)
                        .ok()
                        .flatten()
                        .map(|r| r.revision as u64)
                        .unwrap_or(1);
                    (1, format!("rev:{canonical}"))
                } else {
                    (1, "rev:1".to_string())
                }
            };

            let format = params
                .format
                .as_deref()
                .map(SpilloverFormat::parse_strict)
                .transpose()
                .map_err(anyhow::Error::msg)?;

            let requested_binding = ContinuationBinding {
                workspace_id: binding.workspace_id.clone(),
                tool: "spillover_get".to_string(),
                arguments_hash: String::new(),
                generation,
                source_hashes,
            };

            match store.read(&params.spillover_handle, &requested_binding, format) {
                Ok(page) => {
                    let result = CallToolResult::success(vec![Content::text(format_page(&page))]);
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
                        "spillover_get",
                        start.elapsed(),
                        &report,
                        workspace_snapshot.as_ref(),
                    );
                    return Ok(result);
                }
                Err(failure) => {
                    let req_failure = RequestFailure::new(
                        &failure.code,
                        &failure.message,
                        false,
                        serde_json::json!({ "restart_command": failure.restart_command }),
                    );
                    let message = format!("spillover_get failed: {}", failure);
                    self.record_tool_failure(
                        "spillover_get",
                        start.elapsed(),
                        workspace_snapshot.as_ref(),
                        metadata.clone(),
                        Vec::new(),
                        Self::input_bytes_from_metadata(&metadata),
                        &message,
                    );
                    return Err(anyhow::Error::new(req_failure));
                }
            }
        }

        // 2. Legacy in-memory fallback for non-hex handles (e.g. gc_... / br_...)
        let result = match params.call_tool(self).await {
            Ok(result) => result,
            Err(e) => {
                let message = format!("spillover_get failed: {}", e);
                self.record_tool_failure(
                    "spillover_get",
                    start.elapsed(),
                    workspace_snapshot.as_ref(),
                    metadata.clone(),
                    Vec::new(),
                    Self::input_bytes_from_metadata(&metadata),
                    &message,
                );
                return Err(e);
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
            "spillover_get",
            start.elapsed(),
            &report,
            workspace_snapshot.as_ref(),
        );
        Ok(result)
    }
}
