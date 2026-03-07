// Tests for src/handler.rs — JulieServerHandler construction and lifecycle.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::daemon_state::DaemonState;
use crate::handler::JulieServerHandler;
use anyhow::Result;

#[tokio::test(flavor = "multi_thread")]
async fn handler_construction_sets_workspace_root() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;
    // workspace_root should be set to cwd (the default for new_for_test)
    assert!(handler.workspace_root.is_absolute() || handler.workspace_root.as_os_str() == ".");
    // workspace should start as None (lazy init)
    let ws = handler.workspace.read().await;
    assert!(ws.is_none(), "workspace should be None before initialization");
    Ok(())
}

// ============================================================================
// Task 2: DaemonState on JulieServerHandler
// ============================================================================

#[test]
fn new_sync_sets_daemon_state_to_none() {
    // In stdio mode (the default), daemon_state should be None.
    let handler = JulieServerHandler::new_sync(PathBuf::from("/tmp/test")).unwrap();
    assert!(
        handler.daemon_state.is_none(),
        "stdio-mode handler should have daemon_state = None"
    );
}

#[test]
fn new_with_daemon_state_sets_daemon_state_to_some() {
    // In daemon mode, daemon_state should be Some(...).
    let ds = Arc::new(RwLock::new(DaemonState::new()));
    let handler =
        JulieServerHandler::new_with_daemon_state(PathBuf::from("/tmp/test"), ds.clone()).unwrap();

    assert!(
        handler.daemon_state.is_some(),
        "daemon-mode handler should have daemon_state = Some"
    );
}

#[test]
fn daemon_state_is_shared_across_cloned_handlers() {
    // The handler is Clone (required by rmcp). Cloning should share the
    // same Arc<RwLock<DaemonState>>, not create a separate copy.
    let ds = Arc::new(RwLock::new(DaemonState::new()));
    let handler =
        JulieServerHandler::new_with_daemon_state(PathBuf::from("/tmp/test"), ds.clone()).unwrap();
    let cloned = handler.clone();

    // Both should point to the same Arc
    let original_ptr = Arc::as_ptr(handler.daemon_state.as_ref().unwrap());
    let cloned_ptr = Arc::as_ptr(cloned.daemon_state.as_ref().unwrap());
    assert_eq!(
        original_ptr, cloned_ptr,
        "cloned handler should share the same DaemonState Arc"
    );
}

#[tokio::test]
async fn daemon_state_provides_access_to_workspaces() {
    // Verify that a handler with daemon_state can read the workspaces map.
    let ds = Arc::new(RwLock::new(DaemonState::new()));
    let handler =
        JulieServerHandler::new_with_daemon_state(PathBuf::from("/tmp/test"), ds.clone()).unwrap();

    // Initially empty
    let state = handler.daemon_state.as_ref().unwrap().read().await;
    assert!(
        state.workspaces.is_empty(),
        "fresh DaemonState should have no workspaces"
    );
}
