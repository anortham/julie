//! Request engine application execution and tool dispatching.

use crate::mcp_compat::CallToolResult;
use crate::request_engine::binding::BindingResolver;
use crate::request_engine::catalog::{DecodedTool, ToolCatalog};
use crate::request_engine::runtime_factory::{RequestRuntime, RuntimeFactory};
use crate::request_engine::semantic::{
    DefaultSemanticRuntime, NoopSemanticRuntime, SemanticMode, SemanticReadiness, SemanticRuntime,
};
use crate::request_engine::types::{
    RequestContext, RequestFailure, RequestReadiness, ToolReply, ToolRequest,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

pub struct RequestEngine {
    pub catalog: ToolCatalog,
    pub bindings: BindingResolver,
    pub runtimes: Arc<RuntimeFactory>,
    pub semantic_runtime: Arc<dyn SemanticRuntime>,
    pub service_url: RwLock<Option<String>>,
}

impl RequestEngine {
    pub fn new(bindings: BindingResolver, runtimes: Arc<RuntimeFactory>) -> Self {
        let semantic_runtime = Arc::new(DefaultSemanticRuntime::from_registry_paths(
            runtimes.registry_paths().clone(),
        ));
        Self::with_semantic_runtime(bindings, runtimes, semantic_runtime)
    }

    pub fn with_semantic_runtime(
        bindings: BindingResolver,
        runtimes: Arc<RuntimeFactory>,
        semantic_runtime: Arc<dyn SemanticRuntime>,
    ) -> Self {
        Self {
            catalog: ToolCatalog::new(),
            bindings,
            runtimes,
            semantic_runtime,
            service_url: RwLock::new(None),
        }
    }

    pub fn with_noop_semantics(bindings: BindingResolver, runtimes: Arc<RuntimeFactory>) -> Self {
        Self::with_semantic_runtime(bindings, runtimes, Arc::new(NoopSemanticRuntime))
    }

    pub fn set_service_url(&self, url: String) {
        if let Ok(mut guard) = self.service_url.write() {
            *guard = Some(url);
        }
    }

    pub fn service_url(&self) -> Option<String> {
        self.service_url.read().ok().and_then(|g| g.clone())
    }

    pub fn with_service_url(self, url: String) -> Self {
        self.set_service_url(url);
        self
    }

    pub async fn execute(
        &self,
        request: ToolRequest,
        context: RequestContext,
    ) -> Result<ToolReply, RequestFailure> {
        // Step 1: Cooperative cancellation check
        context.check_cancelled()?;

        // Step 2: Typed decoding (fails fast before I/O on malformed/unknown args)
        let raw_arguments = request.arguments.clone();
        let decoded = ToolCatalog::decode(&request.name, request.arguments)?;

        // Special check: dashboard requires service URL or foreground
        if let DecodedTool::ManageWorkspace(ref p) = decoded {
            if p.operation == "dashboard" {
                if let Some(url) = self.service_url() {
                    let readiness = RequestReadiness::ready(request.semantics);
                    return Ok(ToolReply::from_result(
                        "manage_workspace",
                        None,
                        serde_json::json!({ "url": url }),
                        readiness,
                    ));
                }
                return Err(RequestFailure::foreground_required(
                    "Interactive dashboard requires foreground mode. Run with --foreground flag.",
                ));
            }
        }

        // Step 3: Workspace binding resolution
        let binding = match self.bindings.resolve(
            request.workspace,
            decoded.workspace().map(PathBuf::from),
            decoded.is_unbound(),
        ) {
            Ok(b) => b,
            Err(e) => {
                if let Some(handler) = self.runtimes.template_handler() {
                    let snapshot = handler.require_primary_workspace_binding().ok();
                    let metadata = serde_json::Value::Object(raw_arguments.clone());
                    let file_paths = raw_arguments
                        .get("file_path")
                        .or_else(|| raw_arguments.get("context_file"))
                        .and_then(|v| v.as_str())
                        .map(|s| vec![s.to_string()])
                        .unwrap_or_default();
                    handler.record_tool_failure(
                        &request.name,
                        std::time::Duration::from_millis(0),
                        snapshot.as_ref(),
                        metadata,
                        file_paths,
                        None,
                        &e.message,
                    );
                }
                return Err(e);
            }
        };

        // Step 4: Runtime acquisition
        let runtime = self.runtimes.acquire(binding.as_ref(), &context).await?;

        // Step 5: Follower access check
        let access = match &decoded {
            DecodedTool::ManageWorkspace(params)
                if params.operation == "recover_edit" || params.operation == "recover-edit" =>
            {
                crate::request_engine::types::AccessClass::SourceEdit
            }
            _ => decoded.access(),
        };
        runtime.check_access(access)?;

        // Step 6: Semantic readiness check
        let semantic_req = decoded.semantic_requirement();
        let semantic_readiness = match binding.as_ref() {
            Some(b) if !semantic_req.is_none() => {
                self.semantic_runtime
                    .ensure_ready(
                        b,
                        semantic_req,
                        request.semantics,
                        context.deadline,
                        &context.cancellation,
                    )
                    .await?
            }
            _ => SemanticReadiness::Disabled,
        };

        if let SemanticReadiness::Ready { .. } = semantic_readiness {
            if let Some(provider) = self.semantic_runtime.provider() {
                runtime
                    .handler()
                    .set_injected_embedding_provider(Some(provider));
            }
            runtime
                .handler()
                .semantics_disabled
                .store(false, Ordering::Release);
        } else if request.semantics == SemanticMode::Off {
            runtime.handler().set_injected_embedding_provider(None);
            runtime
                .handler()
                .semantics_disabled
                .store(true, Ordering::Release);
        } else {
            runtime
                .handler()
                .semantics_disabled
                .store(false, Ordering::Release);
        }

        // Step 7: Dispatch tool execution
        let result = self
            .dispatch(decoded, &runtime, &context, request.semantics)
            .await?;

        // Step 8: Envelope construction and normalization
        let readiness = semantic_readiness.to_request_readiness(request.semantics);
        let workspace_id = binding.map(|b| b.workspace_id);
        let value = serde_json::to_value(&result)
            .map_err(|e| RequestFailure::internal(format!("Failed to serialize result: {e}")))?;

        Ok(ToolReply::from_result(
            request.name,
            workspace_id,
            value,
            readiness,
        ))
    }

    async fn dispatch(
        &self,
        decoded: DecodedTool,
        runtime: &RequestRuntime,
        context: &RequestContext,
        semantics_mode: SemanticMode,
    ) -> Result<CallToolResult, RequestFailure> {
        // Mirror cancellation into AtomicBool for syntax-api cooperation
        let cancelled = Arc::new(AtomicBool::new(context.cancellation.is_cancelled()));
        let c_clone = Arc::clone(&cancelled);
        let token = context.cancellation.clone();
        tokio::spawn(async move {
            token.cancelled().await;
            c_clone.store(true, Ordering::Release);
        });
        let budget = julie_core::embeddings_contract::EmbeddingRequestBudget::new(
            context.to_std_deadline(),
            Arc::clone(&cancelled),
        );

        let handler = runtime.handler();

        let core_mode = match semantics_mode {
            SemanticMode::Auto => julie_core::embeddings_contract::SemanticMode::Auto,
            SemanticMode::Off => julie_core::embeddings_contract::SemanticMode::Off,
            SemanticMode::Required => julie_core::embeddings_contract::SemanticMode::Required,
        };

        let result = match decoded {
            DecodedTool::BlastRadius(p) => handler.execute_blast_radius(p).await,
            DecodedTool::CallPath(p) => handler.execute_call_path(p).await,
            DecodedTool::DeepDive(mut p) => {
                p.semantics = Some(core_mode);
                handler.execute_deep_dive(p).await
            }
            DecodedTool::EditFile(p) => {
                handler
                    .execute_edit_file_with_context(
                        p,
                        context.to_std_deadline(),
                        &context.cancellation,
                    )
                    .await
            }
            DecodedTool::FastRefs(mut p) => {
                p.semantics = Some(core_mode);
                handler.execute_fast_refs_with_budget(p, budget).await
            }
            DecodedTool::FastSearch(mut p) => {
                p.search.semantics = Some(core_mode);
                handler.execute_fast_search_with_budget(p, budget).await
            }
            DecodedTool::GetContext(mut p) => {
                p.semantics = Some(core_mode);
                handler.execute_get_context_with_budget(p, budget).await
            }
            DecodedTool::GetSymbols(p) => handler.execute_get_symbols(p).await,
            DecodedTool::ManageWorkspace(p) => handler.execute_manage_workspace(p).await,
            DecodedTool::Patterns(p) => handler.execute_patterns(p).await,
            DecodedTool::RenameSymbol(p) => {
                handler
                    .execute_rename_symbol_with_context(
                        p,
                        context.to_std_deadline(),
                        &context.cancellation,
                    )
                    .await
            }
            DecodedTool::RewriteSymbol(p) => {
                handler
                    .execute_rewrite_symbol_with_context(
                        p,
                        context.to_std_deadline(),
                        &context.cancellation,
                    )
                    .await
            }
            DecodedTool::SpilloverGet(p) => handler.execute_spillover_get(p).await,
        };

        result.map_err(|e| {
            if let Some(failure) = e.downcast_ref::<RequestFailure>() {
                failure.clone()
            } else if context.cancellation.is_cancelled()
                || e.to_string().to_lowercase().contains("cancelled")
            {
                RequestFailure::cancelled(e.to_string())
            } else if tokio::time::Instant::now() >= context.deadline
                || e.to_string().to_lowercase().contains("deadline exceeded")
                || e.to_string().to_lowercase().contains("timed out")
            {
                RequestFailure::deadline_exceeded(e.to_string())
            } else if e.to_string().contains("SEMANTICS_NOT_READY")
                || e.to_string().to_lowercase().contains("semantics not ready")
                || e.to_string()
                    .to_lowercase()
                    .contains("refusing file embedding: no ready embedding generation exists")
            {
                RequestFailure::semantics_not_ready(e.to_string(), serde_json::json!({}))
            } else {
                RequestFailure::new("TOOL_ERROR", e.to_string(), false, serde_json::json!({}))
            }
        })
    }
}
