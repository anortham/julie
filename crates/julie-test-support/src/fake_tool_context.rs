//! `FakeToolContext` — hermetic test double for `ToolContext`.
//!
//! Used by `julie-tools` handler-free tests (T2b.5+) so the tool layer can be
//! exercised without constructing a real `JulieServerHandler`.
//!
//! Design:
//! - Cheap/identity methods: backed by struct fields, fully configurable.
//! - Heavyweight DB/index methods: path-based injection — if `primary_db_path`
//!   is set a real `FactsStore` is opened on each call; otherwise returns a
//!   descriptive `Err`.  Tests that need a real DB use `julie_core::test_support::db()`
//!   to obtain a tempdir-backed database path and inject it via `with_primary_db_path`.
//! - Embedding methods: `Option<Arc<dyn EmbeddingProvider>>` injection; returns `None`
//!   when unconfigured (matches the trait's `Option` return).
//! - Purpose-methods: configurable defaults (`WorkspaceTarget::Primary`,
//!   `SystemStatus::FullyReady { symbol_count: 0 }`).
//! - **Hermetic**: `require_primary_workspace_root` returns the injected path
//!   verbatim — does NOT walk the filesystem to discover a workspace root.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use julie_context::{ToolContext, WorkspaceTarget};
use julie_core::embeddings_contract::EmbeddingProvider;
use julie_core::health_types::SystemStatus;
use julie_core::mcp_compat::CallToolResult;
use julie_index::search::SearchIndex;
use julie_index::snapshot::Snapshot;

use crate::SnapshotFixture;

/// Hermetic test double for [`ToolContext`].
///
/// Construct with [`FakeToolContext::new()`] (sensible defaults) then chain
/// `with_*` setters to configure the fields your test needs.
pub struct FakeToolContext {
    // ── Identity ────────────────────────────────────────────────────────────
    pub session_id: String,
    pub primary_workspace_id: Option<String>,
    pub loaded_workspace_id_val: Option<String>,
    /// Injected workspace root — returned verbatim by `require_primary_workspace_root`.
    /// No filesystem walk. Tests inject e.g. a `TempDir` path.
    pub primary_workspace_root: Option<PathBuf>,

    // ── DB / index injection ────────────────────────────────────────────────
    /// Path to a SQLite database file. If set, DB methods open a new
    /// `FactsStore` connection on each call. Use `db()` from
    /// `julie_core::test_support` to get a tempdir-backed path.
    pub primary_db_path: Option<PathBuf>,
    /// Workspace-specific SQLite paths used by cross-workspace DB methods.
    pub workspace_db_paths: HashMap<String, PathBuf>,
    /// Optional search index to return from index methods.
    pub primary_search_index: Option<Arc<SearchIndex>>,
    /// Snapshot served for every target by `ToolContext::snapshot`.
    pub snapshot_fixture: Option<Arc<SnapshotFixture>>,

    // ── Embeddings ──────────────────────────────────────────────────────────
    pub embedding_provider_val: Option<Arc<dyn EmbeddingProvider>>,

    // ── Purpose-method config ────────────────────────────────────────────────
    /// Value returned by `resolve_workspace_target` for any workspace_param.
    pub resolved_target: WorkspaceTarget,
    /// Value returned by `system_readiness` for any workspace_id.
    pub system_status: SystemStatus,
}

impl Default for FakeToolContext {
    fn default() -> Self {
        Self {
            session_id: "fake-session".to_string(),
            primary_workspace_id: None,
            loaded_workspace_id_val: None,
            primary_workspace_root: None,
            primary_db_path: None,
            workspace_db_paths: HashMap::new(),
            primary_search_index: None,
            snapshot_fixture: None,
            embedding_provider_val: None,
            resolved_target: WorkspaceTarget::Primary,
            system_status: SystemStatus::FullyReady { symbol_count: 0 },
        }
    }
}

impl FakeToolContext {
    /// Construct with sensible hermetic defaults.
    pub fn new() -> Self {
        Self::default()
    }

    // ── Builder setters ──────────────────────────────────────────────────────

    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = id.into();
        self
    }

    pub fn with_workspace_id(mut self, id: impl Into<String>) -> Self {
        self.primary_workspace_id = Some(id.into());
        self
    }

    pub fn with_loaded_workspace_id(mut self, id: impl Into<String>) -> Self {
        self.loaded_workspace_id_val = Some(id.into());
        self
    }

    /// Set the workspace root path returned by `require_primary_workspace_root`.
    /// Hermetic: the path is used as-is; no filesystem walk.
    pub fn with_primary_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.primary_workspace_root = Some(path.into());
        self
    }

    /// Set the SQLite database file path used by all DB methods.
    pub fn with_primary_db_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.primary_db_path = Some(path.into());
        self
    }

    pub fn with_workspace_db_path(
        mut self,
        workspace_id: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Self {
        self.workspace_db_paths
            .insert(workspace_id.into(), path.into());
        self
    }

    pub fn with_search_index(mut self, index: Arc<SearchIndex>) -> Self {
        self.primary_search_index = Some(index);
        self
    }

    pub fn with_snapshot_fixture(mut self, fixture: SnapshotFixture) -> Self {
        self.snapshot_fixture = Some(Arc::new(fixture));
        self
    }

    pub fn with_embedding_provider(mut self, provider: Arc<dyn EmbeddingProvider>) -> Self {
        self.embedding_provider_val = Some(provider);
        self
    }

    pub fn with_resolved_target(mut self, target: WorkspaceTarget) -> Self {
        self.resolved_target = target;
        self
    }

    pub fn with_system_status(mut self, status: SystemStatus) -> Self {
        self.system_status = status;
        self
    }
}

#[async_trait]
impl ToolContext for FakeToolContext {
    // ── Identity (sync) ──────────────────────────────────────────────────────

    fn current_workspace_id(&self) -> Option<String> {
        self.primary_workspace_id.clone()
    }

    fn require_primary_workspace_identity(&self) -> Result<String> {
        self.primary_workspace_id
            .clone()
            .ok_or_else(|| anyhow!("no primary workspace (FakeToolContext)"))
    }

    fn require_primary_workspace_root(&self) -> Result<PathBuf> {
        self.primary_workspace_root.clone().ok_or_else(|| {
            anyhow!("no primary workspace root (FakeToolContext) — inject via with_primary_root")
        })
    }

    fn loaded_workspace_id(&self) -> Option<String> {
        self.loaded_workspace_id_val.clone()
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    async fn get_workspace_root_for_target(&self, _workspace_id: &str) -> Result<PathBuf> {
        self.primary_workspace_root.clone().ok_or_else(|| {
            anyhow!(
                "FakeToolContext::get_workspace_root_for_target not configured \
                 — inject via with_primary_root"
            )
        })
    }

    async fn snapshot(&self, _target: &WorkspaceTarget) -> Result<Arc<Snapshot>> {
        self.snapshot_fixture
            .as_ref()
            .map(|fixture| fixture.snapshot())
            .ok_or_else(|| {
                anyhow!(
                    "FakeToolContext::snapshot not configured — inject via with_snapshot_fixture"
                )
            })
    }

    // ── Embeddings (async) ───────────────────────────────────────────────────

    async fn embedding_provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        self.embedding_provider_val.clone()
    }

    async fn ensure_embedding_provider(
        &self,
        _timeout: Duration,
    ) -> Option<Arc<dyn EmbeddingProvider>> {
        self.embedding_provider_val.clone()
    }

    // ── Purpose-methods ──────────────────────────────────────────────────────

    async fn resolve_workspace_target(
        &self,
        _workspace_param: Option<&str>,
    ) -> Result<WorkspaceTarget> {
        Ok(self.resolved_target.clone())
    }

    async fn ensure_target_workspace_indexed_if_pending(
        &self,
        _workspace_id: &str,
    ) -> Result<Option<CallToolResult>> {
        Ok(None)
    }

    async fn system_readiness(&self, _target_workspace_id: Option<&str>) -> Result<SystemStatus> {
        Ok(self.system_status.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn defaults_are_sensible() {
        let ctx = FakeToolContext::new();
        assert_eq!(ctx.session_id(), "fake-session");
        assert_eq!(ctx.current_workspace_id(), None);
        assert_eq!(ctx.loaded_workspace_id(), None);
    }

    #[tokio::test]
    async fn builder_setters_work() {
        let ctx = FakeToolContext::new()
            .with_session_id("test-session")
            .with_workspace_id("ws-abc")
            .with_loaded_workspace_id("ws-secondary");

        assert_eq!(ctx.session_id(), "test-session");
        assert_eq!(ctx.current_workspace_id(), Some("ws-abc".to_string()));
        assert_eq!(ctx.loaded_workspace_id(), Some("ws-secondary".to_string()));
        assert_eq!(ctx.require_primary_workspace_identity().unwrap(), "ws-abc");
    }

    #[tokio::test]
    async fn resolve_workspace_target_returns_primary_by_default() {
        let ctx = FakeToolContext::new();
        let target = ctx.resolve_workspace_target(None).await.unwrap();
        assert_eq!(target, WorkspaceTarget::Primary);
    }

    #[tokio::test]
    async fn resolve_workspace_target_configurable() {
        let ctx = FakeToolContext::new()
            .with_resolved_target(WorkspaceTarget::Target("ws-42".to_string()));
        let target = ctx.resolve_workspace_target(Some("ws-42")).await.unwrap();
        assert_eq!(target, WorkspaceTarget::Target("ws-42".to_string()));
    }

    #[tokio::test]
    async fn unconfigured_workspace_root_returns_err() {
        let ctx = FakeToolContext::new();
        assert!(ctx.require_primary_workspace_root().is_err());
        assert!(ctx.get_workspace_root_for_target("ws-1").await.is_err());
    }

    #[tokio::test]
    async fn unconfigured_identity_returns_err() {
        let ctx = FakeToolContext::new();
        let err = ctx.require_primary_workspace_identity().unwrap_err();
        assert!(err.to_string().contains("no primary workspace"));
    }

    #[tokio::test]
    async fn ensure_indexed_always_returns_none() {
        let ctx = FakeToolContext::new();
        let result = ctx
            .ensure_target_workspace_indexed_if_pending("ws-any")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn system_readiness_default_and_configurable() {
        let ctx = FakeToolContext::new();
        assert_eq!(
            ctx.system_readiness(None).await.unwrap(),
            SystemStatus::FullyReady { symbol_count: 0 }
        );

        let ctx2 = FakeToolContext::new().with_system_status(SystemStatus::NotReady);
        assert_eq!(
            ctx2.system_readiness(Some("ws-1")).await.unwrap(),
            SystemStatus::NotReady
        );
    }

    #[tokio::test]
    async fn embedding_provider_unconfigured_is_none() {
        let ctx = FakeToolContext::new();
        assert!(ctx.embedding_provider().await.is_none());
        assert!(
            ctx.ensure_embedding_provider(Duration::from_millis(100))
                .await
                .is_none()
        );
    }
}
