//! Tests for the daemon file watcher module (`daemon_watcher.rs`).
//!
//! Tests cover:
//! - Watcher lifecycle (start, stop, stop_all)
//! - Duplicate start prevention
//! - Active watcher tracking
//! - Integration with DaemonState (start_watchers_for_ready_projects)
//! - Watcher respects ignore patterns and extension filtering

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use tempfile::TempDir;

use crate::daemon_watcher::DaemonWatcherManager;
use crate::database::SymbolDatabase;

/// Helper: create a temporary workspace with a SQLite database.
fn create_test_workspace_with_db() -> (TempDir, Arc<StdMutex<SymbolDatabase>>) {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join(".julie/db/symbols.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = Arc::new(StdMutex::new(SymbolDatabase::new(&db_path).unwrap()));
    (temp_dir, db)
}

#[test]
fn test_watcher_manager_creation() {
    let manager = DaemonWatcherManager::new();
    assert!(manager.is_ok(), "DaemonWatcherManager::new() should succeed");
}

#[tokio::test]
async fn test_start_and_stop_watching() {
    let manager = DaemonWatcherManager::new().unwrap();
    let (temp_dir, db) = create_test_workspace_with_db();

    // Start watching
    manager
        .start_watching(
            "test_ws_1".to_string(),
            temp_dir.path().to_path_buf(),
            db,
            None,
        )
        .await;

    // Verify watcher is active
    let active = manager.active_watchers().await;
    assert_eq!(active.len(), 1);
    assert!(active.contains(&"test_ws_1".to_string()));

    // Stop watching
    manager.stop_watching("test_ws_1").await;

    // Verify watcher is gone
    let active = manager.active_watchers().await;
    assert!(active.is_empty());
}

#[tokio::test]
async fn test_duplicate_start_is_noop() {
    let manager = DaemonWatcherManager::new().unwrap();
    let (temp_dir, db) = create_test_workspace_with_db();

    let path = temp_dir.path().to_path_buf();

    // Start watching twice — second call should be a no-op
    manager
        .start_watching("dup_ws".to_string(), path.clone(), db.clone(), None)
        .await;
    manager
        .start_watching("dup_ws".to_string(), path, db, None)
        .await;

    // Should still have exactly one watcher
    let active = manager.active_watchers().await;
    assert_eq!(active.len(), 1);
}

#[tokio::test]
async fn test_stop_nonexistent_watcher_is_noop() {
    let manager = DaemonWatcherManager::new().unwrap();

    // Should not panic or error
    manager.stop_watching("nonexistent_ws").await;

    let active = manager.active_watchers().await;
    assert!(active.is_empty());
}

#[tokio::test]
async fn test_stop_all() {
    let manager = DaemonWatcherManager::new().unwrap();

    let (temp_dir_1, db_1) = create_test_workspace_with_db();
    let (temp_dir_2, db_2) = create_test_workspace_with_db();

    manager
        .start_watching(
            "ws_a".to_string(),
            temp_dir_1.path().to_path_buf(),
            db_1,
            None,
        )
        .await;
    manager
        .start_watching(
            "ws_b".to_string(),
            temp_dir_2.path().to_path_buf(),
            db_2,
            None,
        )
        .await;

    let active = manager.active_watchers().await;
    assert_eq!(active.len(), 2);

    // Stop all
    manager.stop_all().await;

    let active = manager.active_watchers().await;
    assert!(active.is_empty());
}

#[tokio::test]
async fn test_multiple_projects_independent() {
    let manager = DaemonWatcherManager::new().unwrap();

    let (temp_dir_1, db_1) = create_test_workspace_with_db();
    let (temp_dir_2, db_2) = create_test_workspace_with_db();
    let (temp_dir_3, db_3) = create_test_workspace_with_db();

    manager
        .start_watching(
            "proj_1".to_string(),
            temp_dir_1.path().to_path_buf(),
            db_1,
            None,
        )
        .await;
    manager
        .start_watching(
            "proj_2".to_string(),
            temp_dir_2.path().to_path_buf(),
            db_2,
            None,
        )
        .await;
    manager
        .start_watching(
            "proj_3".to_string(),
            temp_dir_3.path().to_path_buf(),
            db_3,
            None,
        )
        .await;

    assert_eq!(manager.active_watchers().await.len(), 3);

    // Stop one — others should remain
    manager.stop_watching("proj_2").await;

    let active = manager.active_watchers().await;
    assert_eq!(active.len(), 2);
    assert!(active.contains(&"proj_1".to_string()));
    assert!(active.contains(&"proj_3".to_string()));
    assert!(!active.contains(&"proj_2".to_string()));
}

#[tokio::test]
async fn test_start_watching_invalid_path_does_not_crash() {
    let manager = DaemonWatcherManager::new().unwrap();

    let (_, db) = create_test_workspace_with_db();

    // Watch a path that doesn't exist — should log error, not crash
    manager
        .start_watching(
            "bad_ws".to_string(),
            PathBuf::from("/nonexistent/path/to/project"),
            db,
            None,
        )
        .await;

    // No watcher should be active (it failed to start)
    let active = manager.active_watchers().await;
    assert!(
        active.is_empty(),
        "Watcher for nonexistent path should not be active"
    );
}

/// Test that the watcher manager reuses filtering from watcher::filtering.
///
/// This verifies our shared filtering infrastructure works — the daemon
/// watcher uses the same extension set and ignore patterns as the
/// per-session IncrementalIndexer.
#[test]
fn test_shared_filtering_infrastructure() {
    use crate::watcher::filtering;

    let extensions = filtering::build_supported_extensions();
    let patterns = filtering::build_ignore_patterns().unwrap();

    // Core languages should be supported
    assert!(extensions.contains("rs"), "Rust should be supported");
    assert!(extensions.contains("py"), "Python should be supported");
    assert!(extensions.contains("ts"), "TypeScript should be supported");

    // .txt should NOT be supported
    assert!(!extensions.contains("txt"), ".txt should not be watched");

    // node_modules should be ignored
    let has_node_modules = patterns
        .iter()
        .any(|p| p.as_str().contains("node_modules"));
    assert!(has_node_modules, "node_modules should be in ignore patterns");

    // .julie directory should be ignored
    let has_julie = patterns.iter().any(|p| p.as_str().contains(".julie"));
    assert!(has_julie, ".julie directory should be in ignore patterns");
}

/// Test DaemonState integration: start_watchers_for_ready_projects.
///
/// Verifies that watchers are started only for Ready workspaces, not for
/// Registered or Error ones.
#[tokio::test]
async fn test_daemon_state_start_watchers_for_ready_projects() {
    use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
    use crate::workspace::JulieWorkspace;

    let mut state = DaemonState::new();

    // Create a "Ready" workspace with real db
    let (temp_dir, db) = create_test_workspace_with_db();
    let search_index = None; // No search index in this test

    let mut ws = JulieWorkspace::empty_shell(temp_dir.path().to_path_buf());
    ws.db = Some(db);
    ws.search_index = search_index;

    state.workspaces.insert(
        "ready_ws".to_string(),
        LoadedWorkspace {
            workspace: ws,
            status: WorkspaceLoadStatus::Ready,
            path: temp_dir.path().to_path_buf(),
        },
    );

    // Create a "Registered" workspace (no .julie dir)
    let temp_dir_2 = tempfile::tempdir().unwrap();
    state.workspaces.insert(
        "registered_ws".to_string(),
        LoadedWorkspace {
            workspace: JulieWorkspace::empty_shell(temp_dir_2.path().to_path_buf()),
            status: WorkspaceLoadStatus::Registered,
            path: temp_dir_2.path().to_path_buf(),
        },
    );

    // Create an "Error" workspace
    let temp_dir_3 = tempfile::tempdir().unwrap();
    state.workspaces.insert(
        "error_ws".to_string(),
        LoadedWorkspace {
            workspace: JulieWorkspace::empty_shell(temp_dir_3.path().to_path_buf()),
            status: WorkspaceLoadStatus::Error("load failed".to_string()),
            path: temp_dir_3.path().to_path_buf(),
        },
    );

    // Start watchers for Ready projects
    state.start_watchers_for_ready_projects().await;

    // Only the Ready workspace should have a watcher
    let active = state.watcher_manager.active_watchers().await;
    assert_eq!(active.len(), 1, "Only Ready workspaces should get watchers");
    assert!(
        active.contains(&"ready_ws".to_string()),
        "ready_ws should have an active watcher"
    );

    // Cleanup
    state.watcher_manager.stop_all().await;
}

/// Test that remove_workspace stops the watcher.
#[tokio::test]
async fn test_remove_workspace_stops_watcher() {
    use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
    use crate::workspace::JulieWorkspace;

    let mut state = DaemonState::new();

    // Create a Ready workspace
    let (temp_dir, db) = create_test_workspace_with_db();
    let mut ws = JulieWorkspace::empty_shell(temp_dir.path().to_path_buf());
    ws.db = Some(db);

    state.workspaces.insert(
        "to_remove".to_string(),
        LoadedWorkspace {
            workspace: ws,
            status: WorkspaceLoadStatus::Ready,
            path: temp_dir.path().to_path_buf(),
        },
    );

    // Start watcher
    state.start_watchers_for_ready_projects().await;
    let active = state.watcher_manager.active_watchers().await;
    assert_eq!(active.len(), 1);

    // Remove workspace — should stop watcher
    state.remove_workspace("to_remove").await;

    let active = state.watcher_manager.active_watchers().await;
    assert!(
        active.is_empty(),
        "Watcher should be stopped after workspace removal"
    );
}
