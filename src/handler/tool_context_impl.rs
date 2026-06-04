//! `impl ToolContext for JulieServerHandler` — Phase 2 PR 2b T2b.2.
//!
//! Bridges the 18-method `ToolContext` facade (in `julie-context`) to the
//! existing `JulieServerHandler` inherent methods and fields. Accessor bodies
//! are verbatim delegations; the three purpose-methods encapsulate logic that
//! names top-crate types (`ManageWorkspaceTool`, `DaemonDatabase`, health
//! checker) so that `julie-tools` never needs to depend on them.
//!
//! Retention invariant: `require_primary_workspace_binding` and
//! `primary_workspace_snapshot` are NOT in the trait (U5 nameability sweep
//! dropped them). They MUST stay as inherent `pub` methods on
//! `JulieServerHandler` — handler-bound tests and the 2c `route.rs` caller
//! use them directly. Do not add them to the trait.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use julie_context::{SpilloverStore, ToolContext, WorkspaceTarget};
use julie_core::database::SymbolDatabase;
use julie_core::embeddings_contract::EmbeddingProvider;
use julie_core::health_types::SystemStatus;
use julie_core::mcp_compat::CallToolResult;
use julie_index::search::SearchIndex;

use crate::handler::JulieServerHandler;

#[async_trait::async_trait]
impl ToolContext for JulieServerHandler {
    // ── Identity (sync) ──────────────────────────────────────────────────

    fn current_workspace_id(&self) -> Option<String> {
        // Delegates to the inherent method of the same name.
        JulieServerHandler::current_workspace_id(self)
    }

    fn require_primary_workspace_identity(&self) -> Result<String> {
        JulieServerHandler::require_primary_workspace_identity(self)
    }

    fn require_primary_workspace_root(&self) -> Result<PathBuf> {
        JulieServerHandler::require_primary_workspace_root(self)
    }

    fn loaded_workspace_id(&self) -> Option<String> {
        JulieServerHandler::loaded_workspace_id(self)
    }

    fn is_primary_workspace_swap_in_progress(&self) -> bool {
        JulieServerHandler::is_primary_workspace_swap_in_progress(self)
    }

    /// NEW accessor — wraps the raw `session_metrics.session_id` field.
    fn session_id(&self) -> &str {
        &self.session_metrics.session_id
    }

    /// NEW accessor — wraps the raw `Arc<SpilloverStore>` field.
    fn spillover_store(&self) -> Arc<SpilloverStore> {
        Arc::clone(&self.spillover_store)
    }

    // ── Primary db / index (async) ───────────────────────────────────────

    async fn primary_pooled_database(&self) -> Result<SymbolDatabase> {
        JulieServerHandler::primary_pooled_database(self).await
    }

    async fn primary_pooled_database_and_search_index(
        &self,
    ) -> Result<(SymbolDatabase, Arc<Mutex<SearchIndex>>)> {
        JulieServerHandler::primary_pooled_database_and_search_index(self).await
    }

    // ── Cross-workspace (async) ──────────────────────────────────────────

    async fn get_pooled_database_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<SymbolDatabase> {
        JulieServerHandler::get_pooled_database_for_workspace(self, workspace_id).await
    }

    async fn get_database_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Arc<Mutex<SymbolDatabase>>> {
        JulieServerHandler::get_database_for_workspace(self, workspace_id).await
    }

    async fn get_search_index_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<Arc<Mutex<SearchIndex>>>> {
        JulieServerHandler::get_search_index_for_workspace(self, workspace_id).await
    }

    async fn get_workspace_root_for_target(&self, workspace_id: &str) -> Result<PathBuf> {
        JulieServerHandler::get_workspace_root_for_target(self, workspace_id).await
    }

    // ── Embeddings (async) ───────────────────────────────────────────────

    async fn embedding_provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        JulieServerHandler::embedding_provider(self).await
    }

    /// Purpose-method: encapsulates `wait_for_embedding_provider_settled`
    /// (daemon `EmbeddingServiceSettled` wait + stdio lazy-init).
    async fn ensure_embedding_provider(
        &self,
        timeout: Duration,
    ) -> Option<Arc<dyn EmbeddingProvider>> {
        crate::handler::embedding_init::wait_for_embedding_provider_settled(self, timeout)
            .await
    }

    // ── Purpose-methods (top-crate impls) ────────────────────────────────

    /// Encapsulates the full `resolve_workspace_filter` resolver verbatim,
    /// including the `activate_workspace_with_root` mutation (Blocker B2).
    async fn resolve_workspace_target(
        &self,
        workspace_param: Option<&str>,
    ) -> Result<WorkspaceTarget> {
        crate::handler::workspace_resolution::resolve_workspace_filter(workspace_param, self).await
    }

    /// Encapsulates the `ManageWorkspaceTool` invocation (Blocker B1):
    /// the only production site that instantiates `ManageWorkspaceTool` from
    /// within the tool layer. After T2b.5, tools call this through the trait
    /// instead of reaching for the concrete type.
    async fn ensure_target_workspace_indexed_if_pending(
        &self,
        workspace_id: &str,
    ) -> Result<Option<CallToolResult>> {
        let Some(daemon_db) = self.daemon_db.as_ref() else {
            return Ok(None);
        };
        let Some(row) = daemon_db.get_workspace(workspace_id)? else {
            return Ok(None);
        };
        if row.status == "ready" {
            return Ok(None);
        }

        let session_target_is_active = self.is_workspace_active(workspace_id).await
            || self.loaded_workspace_id().as_deref() == Some(workspace_id);
        if !session_target_is_active {
            return Ok(None);
        }

        let index_tool = crate::tools::ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(row.path),
            name: None,
            workspace_id: None,
            force: Some(false),
            detailed: None,
        };
        let result = index_tool.call_tool_with_options(self, true).await?;
        if result.is_error.unwrap_or(false) {
            return Ok(Some(result));
        }

        Ok(None)
    }

    /// Top-crate purpose-method: delegates to `HealthChecker::check_system_readiness`
    /// which reads `embedding_service.is_some()` (daemon-only field, Blocker B5).
    async fn system_readiness(
        &self,
        target_workspace_id: Option<&str>,
    ) -> Result<SystemStatus> {
        crate::health::HealthChecker::check_system_readiness(self, target_workspace_id).await
    }
}
