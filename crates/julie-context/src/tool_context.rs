//! `ToolContext` — the facade trait that tool-layer code uses in place of
//! `&JulieServerHandler`. Defined here in `julie-context` so that
//! `julie-tools` can depend on just this crate (core + index) rather than
//! pulling in the full top-crate handler, daemon, and runtime.
//!
//! The 18-method surface is the validated U5 result from the PR 2b boundary
//! re-validation. Do NOT add methods without a plan doc update — the surface
//! is deliberately minimal (every addition grows the `FakeToolContext` stub
//! in `julie-test-support` and requires T2b.2 + T2b.3 updates).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;

use julie_core::database::SymbolDatabase;
use julie_core::embeddings_contract::EmbeddingProvider;
use julie_core::health_types::SystemStatus;
use julie_core::mcp_compat::CallToolResult;
use julie_index::search::SearchIndex;

use crate::spillover::SpilloverStore;
use crate::workspace_target::WorkspaceTarget;

#[async_trait::async_trait]
pub trait ToolContext: Send + Sync {
    // ── Identity (sync) ──────────────────────────────────────────────────

    /// Returns the current primary workspace ID, if one is bound.
    fn current_workspace_id(&self) -> Option<String>;

    /// Returns the primary workspace ID, or an error if no workspace is bound.
    fn require_primary_workspace_identity(&self) -> Result<String>;

    /// Returns the primary workspace root path, or an error if no workspace is bound.
    fn require_primary_workspace_root(&self) -> Result<PathBuf>;

    /// Returns the loaded (non-primary / secondary) workspace ID, if any.
    fn loaded_workspace_id(&self) -> Option<String>;

    /// Returns `true` while a primary workspace swap is in progress.
    fn is_primary_workspace_swap_in_progress(&self) -> bool;

    /// Returns the session ID for this handler instance.
    ///
    /// NEW accessor wrapping the raw `session_metrics.session_id` field.
    /// Callers that need an owned `String` call `.to_string()`.
    fn session_id(&self) -> &str;

    /// Returns the spillover store for this session.
    ///
    /// NEW accessor wrapping the raw `Arc<SpilloverStore>` field.
    fn spillover_store(&self) -> Arc<SpilloverStore>;

    // ── Primary db / index (async) ───────────────────────────────────────

    /// Returns an owned `SymbolDatabase` wrapping a pooled connection for the
    /// primary workspace.
    async fn primary_pooled_database(&self) -> Result<SymbolDatabase>;

    /// Returns an owned `SymbolDatabase` plus the primary workspace's
    /// `SearchIndex`, or an error if the index has not been initialized.
    async fn primary_pooled_database_and_search_index(
        &self,
    ) -> Result<(SymbolDatabase, Arc<SearchIndex>)>;

    // ── Cross-workspace (async) ──────────────────────────────────────────

    /// Returns an owned `SymbolDatabase` wrapping a pooled connection for
    /// the given workspace ID.
    async fn get_pooled_database_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<SymbolDatabase>;

    /// Returns a shared `Arc<Mutex<SymbolDatabase>>` for the given workspace ID.
    ///
    /// Prefer `get_pooled_database_for_workspace` for new code; this variant
    /// serializes all callers through the mutex.
    async fn get_database_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Arc<Mutex<SymbolDatabase>>>;

    /// Returns the `SearchIndex` for the given workspace ID, or `None` if the
    /// index has not been created yet.
    async fn get_search_index_for_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<Arc<SearchIndex>>>;

    /// Returns the on-disk root path for the given workspace ID.
    async fn get_workspace_root_for_target(&self, workspace_id: &str) -> Result<PathBuf>;

    // ── Embeddings (async) ───────────────────────────────────────────────

    /// Returns the embedding provider, preferring the daemon shared service
    /// over the per-workspace provider. Returns `None` if no provider is
    /// available (embeddings not initialized).
    async fn embedding_provider(&self) -> Option<Arc<dyn EmbeddingProvider>>;

    /// Waits up to `timeout` for the embedding provider to settle (daemon
    /// cold-start) and returns it. Encapsulates the daemon
    /// `EmbeddingServiceSettled` wait and the stdio lazy-init path.
    async fn ensure_embedding_provider(
        &self,
        timeout: Duration,
    ) -> Option<Arc<dyn EmbeddingProvider>>;

    // ── Purpose-methods (top-crate impls; encapsulate daemon/tool types) ─

    /// Resolve a workspace parameter string to a `WorkspaceTarget`.
    ///
    /// - `None` / `"primary"` → `WorkspaceTarget::Primary`
    /// - Any other string → validated (daemon mode) or accepted permissively
    ///   (stdio mode) as `WorkspaceTarget::Target(id)`.
    ///
    /// Encapsulates the full `resolve_workspace_filter` resolver, including
    /// the `activate_workspace_with_root` mutation (Blocker B2).
    async fn resolve_workspace_target(
        &self,
        workspace_param: Option<&str>,
    ) -> Result<WorkspaceTarget>;

    /// Ensures the target workspace is indexed if an index was requested but
    /// not yet completed. Returns `Some(result)` if the caller should
    /// short-circuit with that result (e.g. an "indexing in progress" message),
    /// or `None` if the workspace is ready and the tool may proceed.
    ///
    /// Encapsulates the `ManageWorkspaceTool` invocation (Blocker B1).
    async fn ensure_target_workspace_indexed_if_pending(
        &self,
        workspace_id: &str,
    ) -> Result<Option<CallToolResult>>;

    /// Returns the system readiness status for the given workspace (or the
    /// primary workspace if `None`).
    ///
    /// Top-crate purpose-method: the implementation reads
    /// `embedding_service.is_some()` (daemon-only field, above julie-context),
    /// so this cannot be a default method (Blocker B5, CORRECTED).
    async fn system_readiness(
        &self,
        target_workspace_id: Option<&str>,
    ) -> Result<SystemStatus>;
}
