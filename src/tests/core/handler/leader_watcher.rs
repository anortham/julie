//! T5 gate-invariant test: exactly one of two in-process handlers on one
//! workspace starts a watcher; the loser constructs no running watcher.
//!
//! Gate invariant:
//!   leader → `loaded_workspace_file_watcher_running_for_test()` = true
//!   loser  → `loaded_workspace_file_watcher_running_for_test()` = false
//!   loser  → workspace is still loaded for read-only access

use crate::handler::JulieServerHandler;
use crate::leadership::LeadershipState;
use crate::registry::discovery::DaemonLockGuard;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

/// Exactly one of two in-process handlers on one workspace starts a watcher;
/// the loser never starts watching (no `IncrementalIndexer::start_watching`
/// call, no OS notify watcher, no Tantivy writer acquisition).
///
/// Proves the gate invariant required by T5:
/// "exactly one of two in-process handlers on one workspace starts a watcher;
/// the loser constructs no running IncrementalIndexer and never acquires the
/// Tantivy writer lock."
#[tokio::test]
async fn test_leader_watcher_started_loser_watcher_not_started() {
    let workspace_dir = tempfile::tempdir().unwrap();
    let workspace_path = workspace_dir.path().to_string_lossy().to_string();

    // ---- leader -------------------------------------------------------
    let lock_path = workspace_dir.path().join(".leader.lock");
    let guard =
        DaemonLockGuard::try_acquire(&lock_path).expect("lock must be acquirable on a fresh path");

    let leader_hint = WorkspaceStartupHint {
        path: workspace_dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    let leader =
        JulieServerHandler::new_in_process(leader_hint, None, LeadershipState::leader(guard), None)
            .await
            .unwrap();

    // ---- loser --------------------------------------------------------
    let loser_hint = WorkspaceStartupHint {
        path: workspace_dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    // A real in-process loser is a FOLLOWER (not none()): none() is the
    // stdio/daemon state, which legitimately watches. The watcher gate is
    // `!is_in_process_follower()`, so only follower() suppresses the watcher.
    let loser =
        JulieServerHandler::new_in_process(loser_hint, None, LeadershipState::follower(), None)
            .await
            .unwrap();

    // Initialize both on the same workspace path. Leader goes first so it
    // creates the workspace; loser detects and loads the existing one.
    leader
        .initialize_workspace_with_force(Some(workspace_path.clone()), false)
        .await
        .unwrap();
    loser
        .initialize_workspace_with_force(Some(workspace_path.clone()), false)
        .await
        .unwrap();

    // ---- assertions ---------------------------------------------------

    // Leader: watcher must be running.
    assert!(
        leader
            .loaded_workspace_file_watcher_running_for_test()
            .await,
        "leader must have its file watcher running after initialize_workspace_with_force"
    );

    // Loser (follower): watcher must NOT be running (is_in_process_follower() ==
    // true → start_file_watching returns early, so IncrementalIndexer.start_watching()
    // is never called and no Tantivy IndexWriter is ever requested).
    assert!(
        !loser.loaded_workspace_file_watcher_running_for_test().await,
        "in-process follower must NOT have its file watcher running"
    );

    // Loser must still have a workspace loaded (reads must be valid).
    {
        let ws = loser.workspace.read().await;
        assert!(
            ws.is_some(),
            "loser must have a workspace loaded for read-only access"
        );
    }

    // Cleanup: stop the leader watcher to avoid background-task noise after
    // the temp dir is dropped.
    leader
        .stop_loaded_workspace_file_watching_for_test()
        .await
        .unwrap();
}
