//! Test workspace builders with proper isolation

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

/// Create a unique test workspace with process ID to prevent collisions
pub fn create_unique_test_workspace(test_name: &str) -> Result<TempDir> {
    let unique_id = format!("{}_{}", test_name, std::process::id());
    let temp_dir = tempfile::Builder::new().prefix(&unique_id).tempdir()?;
    Ok(temp_dir)
}

/// Create an isolated temp workspace root that `find_workspace_root` resolves to itself.
///
/// `find_workspace_root` walks UP from a path looking for repo markers
/// (`.git`, `.vscode`, `Cargo.toml`, `package.json`, `.project`) and returns
/// the first ancestor that has one, or the start path if none is found. Real
/// workspaces always carry such a marker at their root, so resolution stops at
/// the workspace. A bare temp dir under `$TMPDIR` has none, so on a polluted
/// dev box the walk climbs into shared ancestors — e.g. a stray
/// `/private/tmp/Cargo.toml` left by scratch work resolves every test workspace
/// to `tmp_*` instead of its real id, breaking rebind / primary-root assertions
/// (passes on clean CI, fails locally). Dropping an empty `.git` marker at the
/// workspace root keeps these fixtures hermetic to whatever lives above the
/// system temp dir. (#33)
pub fn make_isolated_workspace_root(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    std::fs::create_dir_all(&root).expect("create temp workspace root");
    mark_workspace_root(root.as_path());
    root
}

/// Drop a workspace root marker (`.git`) on an existing directory so
/// `find_workspace_root` resolves it to itself instead of walking up into
/// `$TMPDIR` ancestors. Use this when the workspace root is the `TempDir` path
/// itself (stdio-style `JulieServerHandler::new(temp_dir.path())` tests) rather
/// than a named subdirectory. See [`make_isolated_workspace_root`]. (#33)
pub fn mark_workspace_root(dir: &Path) {
    std::fs::create_dir_all(dir.join(".git")).expect("create workspace root marker");
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
    ));

    let mut handler = JulieServerHandler::new(workspace_root).await?;
    handler.daemon_db = Some(daemon_db);
    handler.workspace_pool = Some(pool);

    Ok(IsolatedStorageHandler { handler, temp_home })
}
