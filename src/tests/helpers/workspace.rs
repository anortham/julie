//! Test workspace builders with proper isolation

use crate::handler::JulieServerHandler;
use crate::registry::database::DaemonDatabase;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Create a unique test workspace with process ID to prevent collisions
pub fn create_unique_test_workspace(test_name: &str) -> Result<TempDir> {
    let unique_id = format!("{}_{}", test_name, std::process::id());
    let temp_dir = tempfile::Builder::new().prefix(&unique_id).tempdir()?;
    Ok(temp_dir)
}

// Pure-fs workspace root markers now live in julie-test-support so both the
// top-crate and julie-runtime test binaries can share them without a dep cycle.
pub use julie_test_support::workspace_markers::{
    make_isolated_workspace_root, mark_workspace_root,
};

/// Get fixture path (existing helper, centralized)
pub fn get_fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/test-workspaces")
        .join(name)
}

/// Test handler whose indexes live in a temp daemon home instead of the repo.
pub struct IsolatedStorageHandler {
    pub handler: JulieServerHandler,
    temp_home: TempDir,
}

impl std::ops::Deref for IsolatedStorageHandler {
    type Target = JulieServerHandler;

    fn deref(&self) -> &Self::Target {
        &self.handler
    }
}

impl IsolatedStorageHandler {
    pub fn indexes_dir(&self) -> PathBuf {
        self.temp_home.path().join("indexes")
    }

    pub fn workspace_index_dir(&self, workspace_id: &str) -> PathBuf {
        self.indexes_dir().join(workspace_id)
    }
}

/// Forward `ToolContext` to the inner `JulieServerHandler` so tests can pass
/// `&handler` (where `handler: IsolatedStorageHandler`) to swapped tool entry
/// points that now take `&dyn ToolContext`.
#[async_trait::async_trait]
impl julie_context::ToolContext for IsolatedStorageHandler {
    fn current_workspace_id(&self) -> Option<String> {
        julie_context::ToolContext::current_workspace_id(&self.handler)
    }
    fn require_primary_workspace_identity(&self) -> anyhow::Result<String> {
        julie_context::ToolContext::require_primary_workspace_identity(&self.handler)
    }
    fn require_primary_workspace_root(&self) -> anyhow::Result<PathBuf> {
        julie_context::ToolContext::require_primary_workspace_root(&self.handler)
    }
    fn loaded_workspace_id(&self) -> Option<String> {
        julie_context::ToolContext::loaded_workspace_id(&self.handler)
    }
    fn is_primary_workspace_swap_in_progress(&self) -> bool {
        julie_context::ToolContext::is_primary_workspace_swap_in_progress(&self.handler)
    }
    fn session_id(&self) -> &str {
        julie_context::ToolContext::session_id(&self.handler)
    }
    fn spillover_store(&self) -> Arc<julie_context::SpilloverStore> {
        julie_context::ToolContext::spillover_store(&self.handler)
    }
    async fn primary_pooled_database(
        &self,
    ) -> anyhow::Result<julie_core::database::SymbolDatabase> {
        self.handler.primary_pooled_database().await
    }
    async fn primary_pooled_database_and_search_index(
        &self,
    ) -> anyhow::Result<(
        julie_core::database::SymbolDatabase,
        Arc<julie_index::search::SearchIndex>,
    )> {
        self.handler
            .primary_pooled_database_and_search_index()
            .await
    }
    async fn get_pooled_database_for_workspace(
        &self,
        workspace_id: &str,
    ) -> anyhow::Result<julie_core::database::SymbolDatabase> {
        self.handler
            .get_pooled_database_for_workspace(workspace_id)
            .await
    }
    async fn get_database_for_workspace(
        &self,
        workspace_id: &str,
    ) -> anyhow::Result<Arc<std::sync::Mutex<julie_core::database::SymbolDatabase>>> {
        self.handler.get_database_for_workspace(workspace_id).await
    }
    async fn get_search_index_for_workspace(
        &self,
        workspace_id: &str,
    ) -> anyhow::Result<Option<Arc<julie_index::search::SearchIndex>>> {
        self.handler
            .get_search_index_for_workspace(workspace_id)
            .await
    }
    async fn get_workspace_root_for_target(&self, workspace_id: &str) -> anyhow::Result<PathBuf> {
        self.handler
            .get_workspace_root_for_target(workspace_id)
            .await
    }
    async fn embedding_provider(
        &self,
    ) -> Option<Arc<dyn julie_core::embeddings_contract::EmbeddingProvider>> {
        self.handler.embedding_provider().await
    }
    async fn ensure_embedding_provider(
        &self,
        timeout: std::time::Duration,
    ) -> Option<Arc<dyn julie_core::embeddings_contract::EmbeddingProvider>> {
        self.handler.ensure_embedding_provider(timeout).await
    }
    async fn resolve_workspace_target(
        &self,
        workspace_param: Option<&str>,
    ) -> anyhow::Result<julie_context::WorkspaceTarget> {
        self.handler.resolve_workspace_target(workspace_param).await
    }
    async fn ensure_target_workspace_indexed_if_pending(
        &self,
        workspace_id: &str,
    ) -> anyhow::Result<Option<julie_core::mcp_compat::CallToolResult>> {
        self.handler
            .ensure_target_workspace_indexed_if_pending(workspace_id)
            .await
    }
    async fn system_readiness(
        &self,
        target_workspace_id: Option<&str>,
    ) -> anyhow::Result<julie_core::health_types::SystemStatus> {
        self.handler.system_readiness(target_workspace_id).await
    }
}

/// Create a stdio-style handler that stores indexes in a temp daemon home.
///
/// Useful for tests that need to index the real Julie repo without writing
/// `.julie/indexes` into the workspace under test.
pub async fn create_isolated_storage_handler(
    workspace_root: PathBuf,
) -> Result<IsolatedStorageHandler> {
    let temp_home = tempfile::tempdir()?;
    let daemon_db = Arc::new(DaemonDatabase::open(&temp_home.path().join("daemon.db"))?);

    let mut handler = JulieServerHandler::new(workspace_root).await?;
    handler.daemon_db = Some(daemon_db);

    Ok(IsolatedStorageHandler { handler, temp_home })
}
