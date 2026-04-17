//! Test workspace builders with proper isolation

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
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

/// Create a stdio-style handler that stores indexes in a temp daemon home.
///
/// Useful for tests that need to index the real Julie repo without writing
/// `.julie/indexes` into the workspace under test.
pub async fn create_isolated_storage_handler(
    workspace_root: PathBuf,
) -> Result<IsolatedStorageHandler> {
    let temp_home = tempfile::tempdir()?;
    let daemon_db = Arc::new(DaemonDatabase::open(&temp_home.path().join("daemon.db"))?);
    let indexes_dir = temp_home.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;

    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let mut handler = JulieServerHandler::new(workspace_root).await?;
    handler.daemon_db = Some(daemon_db);
    handler.workspace_pool = Some(pool);

    Ok(IsolatedStorageHandler { handler, temp_home })
}
