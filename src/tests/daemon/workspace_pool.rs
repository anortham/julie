use std::sync::Arc;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;

fn temp_indexes_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

fn temp_workspace_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    // Create .julie directory structure so JulieWorkspace::initialize succeeds
    std::fs::create_dir_all(dir.path().join(".julie")).expect("Failed to create .julie dir");
    dir
}

#[tokio::test]
async fn test_get_or_init_creates_workspace_on_first_call() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None);

    let ws = pool
        .get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    // Workspace should have db and search_index initialized
    assert!(ws.db.is_some(), "db should be initialized");
    assert!(
        ws.search_index.is_some(),
        "search_index should be initialized"
    );
}

#[tokio::test]
async fn test_get_or_init_returns_same_instance_on_second_call() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None);

    let ws1 = pool
        .get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("first get_or_init should succeed");

    let ws2 = pool
        .get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("second get_or_init should succeed");

    // Both should point to the same Arc (same db instance)
    let db1 = ws1.db.as_ref().expect("db1 should exist");
    let db2 = ws2.db.as_ref().expect("db2 should exist");
    assert!(
        Arc::ptr_eq(db1, db2),
        "should return the same workspace instance"
    );
}

#[tokio::test]
async fn test_get_returns_none_for_unknown_workspace() {
    let indexes_dir = temp_indexes_dir();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None);

    let result = pool.get("nonexistent").await;
    assert!(
        result.is_none(),
        "get should return None for unknown workspace"
    );
}

#[tokio::test]
async fn test_get_returns_some_after_init() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None);

    // Initialize workspace
    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    // Now get should return Some
    let ws = pool.get("test_ws").await;
    assert!(ws.is_some(), "get should return Some after init");
}

#[tokio::test]
async fn test_is_indexed_returns_false_before_indexing() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None);

    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    assert!(
        !pool.is_indexed("test_ws").await,
        "should not be indexed initially"
    );
}

#[tokio::test]
async fn test_mark_indexed() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None);

    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    pool.mark_indexed("test_ws").await;
    assert!(
        pool.is_indexed("test_ws").await,
        "should be indexed after mark_indexed"
    );
}

#[tokio::test]
async fn test_active_workspace_count() {
    let indexes_dir = temp_indexes_dir();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None);

    assert_eq!(
        pool.active_count().await,
        0,
        "should start with 0 workspaces"
    );

    let root1 = temp_workspace_root();
    pool.get_or_init("ws1", root1.path().to_path_buf())
        .await
        .expect("first init should succeed");
    assert_eq!(pool.active_count().await, 1);

    let root2 = temp_workspace_root();
    pool.get_or_init("ws2", root2.path().to_path_buf())
        .await
        .expect("second init should succeed");
    assert_eq!(pool.active_count().await, 2);

    // Re-init of existing workspace should not increase count
    pool.get_or_init("ws1", root1.path().to_path_buf())
        .await
        .expect("re-init should succeed");
    assert_eq!(pool.active_count().await, 2);
}

#[tokio::test]
async fn test_concurrent_get_or_init_different_workspaces() {
    let indexes_dir = temp_indexes_dir();
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None));

    let root1 = temp_workspace_root();
    let root2 = temp_workspace_root();
    let root1_path = root1.path().to_path_buf();
    let root2_path = root2.path().to_path_buf();

    let pool1 = pool.clone();
    let pool2 = pool.clone();

    let (r1, r2) = tokio::join!(
        tokio::spawn(async move { pool1.get_or_init("ws_a", root1_path).await }),
        tokio::spawn(async move { pool2.get_or_init("ws_b", root2_path).await }),
    );

    let ws_a = r1
        .expect("task 1 should not panic")
        .expect("ws_a init should succeed");
    let ws_b = r2
        .expect("task 2 should not panic")
        .expect("ws_b init should succeed");

    // Both should be different workspaces
    let db_a = ws_a.db.as_ref().expect("db_a should exist");
    let db_b = ws_b.db.as_ref().expect("db_b should exist");
    assert!(
        !Arc::ptr_eq(db_a, db_b),
        "different workspaces should have different db instances"
    );

    assert_eq!(pool.active_count().await, 2);
}

#[tokio::test]
async fn test_workspace_pool_accepts_daemon_db() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());
    let indexes_dir = tmp.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir).unwrap();

    // Constructor must accept daemon_db -- pool starts empty
    let pool = WorkspacePool::new(indexes_dir.clone(), Some(daemon_db.clone()), None);
    assert_eq!(pool.active_count().await, 0);
}

#[test]
fn test_daemon_db_upsert_on_workspace_init() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap();

    daemon_db.upsert_workspace("test_ws", "/tmp/test", "pending").unwrap();
    daemon_db.update_workspace_status("test_ws", "ready").unwrap();

    let ws = daemon_db.get_workspace("test_ws").unwrap().unwrap();
    assert_eq!(ws.status, "ready");
}

#[tokio::test]
async fn test_watcher_pool_ref_incremented_on_get_or_init() {
    use std::time::Duration;
    use crate::daemon::watcher_pool::WatcherPool;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let pool = WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        None,
        Some(Arc::clone(&watcher_pool)),
    );

    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    // attach() was called, so ref_count should be 1
    assert_eq!(watcher_pool.ref_count("test_ws").await, 1);
}

#[tokio::test]
async fn test_watcher_pool_detached_on_disconnect() {
    use std::time::Duration;
    use crate::daemon::watcher_pool::WatcherPool;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let pool = WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        None,
        Some(Arc::clone(&watcher_pool)),
    );

    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    pool.disconnect_session("test_ws").await;

    // ref_count hit 0, grace deadline should now be set
    assert!(watcher_pool.has_grace_deadline("test_ws").await);
}
