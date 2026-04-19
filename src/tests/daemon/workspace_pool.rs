use std::sync::Arc;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::workspace::registry::generate_workspace_id;

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None, None, None);

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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        None,
        None,
        None,
    ));

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
    let pool = WorkspacePool::new(indexes_dir.clone(), Some(daemon_db.clone()), None, None);
    assert_eq!(pool.active_count().await, 0);
}

#[tokio::test]
async fn test_get_or_init_migrates_project_local_index_to_shared_indexes() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());
    let indexes_dir = tmp.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir).unwrap();

    let workspace_root = temp_workspace_root();
    crate::workspace::JulieWorkspace::initialize(workspace_root.path().to_path_buf())
        .await
        .unwrap();
    let workspace_id = generate_workspace_id(&workspace_root.path().to_string_lossy()).unwrap();
    let project_index = workspace_root
        .path()
        .join(".julie")
        .join("indexes")
        .join(&workspace_id);

    let pool = WorkspacePool::new(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    );

    pool.get_or_init(&workspace_id, workspace_root.path().to_path_buf())
        .await
        .expect("workspace init should reconcile per-project index");

    assert!(
        indexes_dir
            .join(&workspace_id)
            .join("db/symbols.db")
            .exists(),
        "shared index should exist after migration"
    );
    assert!(
        !project_index.exists(),
        "per-project duplicate should be removed after migration"
    );
}

#[test]
fn test_daemon_db_upsert_on_workspace_init() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap();

    daemon_db
        .upsert_workspace("test_ws", "/tmp/test", "pending")
        .unwrap();
    daemon_db
        .update_workspace_status("test_ws", "ready")
        .unwrap();

    let ws = daemon_db.get_workspace("test_ws").unwrap().unwrap();
    assert_eq!(ws.status, "ready");
}

#[tokio::test]
async fn test_watcher_pool_ref_incremented_on_get_or_init() {
    use crate::daemon::watcher_pool::WatcherPool;
    use std::time::Duration;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let pool = WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        None,
        Some(Arc::clone(&watcher_pool)),
        None,
    );

    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    // attach() was called, so ref_count should be 1
    assert_eq!(watcher_pool.ref_count("test_ws").await, 1);
}

#[tokio::test]
async fn test_watcher_pool_reuses_session_refs_and_starts_grace_on_last_disconnect() {
    use crate::daemon::watcher_pool::WatcherPool;
    use std::time::Duration;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let pool = WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        None,
        Some(Arc::clone(&watcher_pool)),
        None,
    );

    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("first get_or_init should succeed");
    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("second get_or_init should reuse the workspace");

    assert_eq!(
        watcher_pool.ref_count("test_ws").await,
        2,
        "shared sessions should increment the same watcher ref count"
    );
    assert!(
        !watcher_pool.has_grace_deadline("test_ws").await,
        "grace should stay off while at least one session is attached"
    );

    pool.disconnect_session("test_ws").await;
    assert_eq!(
        watcher_pool.ref_count("test_ws").await,
        1,
        "disconnecting one session should keep the shared watcher alive"
    );
    assert!(
        !watcher_pool.has_grace_deadline("test_ws").await,
        "grace should not start until the last session disconnects"
    );

    pool.disconnect_session("test_ws").await;
    assert_eq!(
        watcher_pool.ref_count("test_ws").await,
        0,
        "last disconnect should drain the watcher ref count"
    );
    assert!(
        watcher_pool.has_grace_deadline("test_ws").await,
        "last disconnect should start the watcher grace window"
    );
}

// ── D-C1 ─────────────────────────────────────────────────────────────────────
// session_count must NOT be incremented when init_workspace fails, otherwise
// the count stays +1 permanently (leaked) and daemon.db is inconsistent.
#[tokio::test]
async fn test_session_count_not_incremented_on_init_failure() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());
    let indexes_dir = tmp.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir).unwrap();

    // Use a regular FILE as the workspace root so `create_dir_all(.julie)` fails
    let fake_root = tmp.path().join("not_a_dir");
    std::fs::write(&fake_root, b"I am a file").unwrap();

    let pool = WorkspacePool::new(indexes_dir, Some(Arc::clone(&daemon_db)), None, None);

    let result = pool.get_or_init("leak_test_ws", fake_root).await;
    assert!(
        result.is_err(),
        "init should fail when workspace root is a regular file"
    );

    // After fix: session_count stays 0 (increment never happened)
    // Before fix: session_count would be 1 (leaked on failed init)
    if let Ok(Some(row)) = daemon_db.get_workspace("leak_test_ws") {
        assert_eq!(
            row.session_count, 0,
            "session count must not be incremented when init_workspace fails"
        );
    }
    // If the row doesn't exist at all, that's also acceptable — no leak either way
}

// ── D-H6 ─────────────────────────────────────────────────────────────────────
// After indexing, the IPC session tear-down must call sync_indexed_from_db so
// the pool's in-memory `indexed` flag reflects what daemon.db already knows.
#[tokio::test]
async fn test_sync_indexed_from_db_sets_flag_when_ready() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();

    let pool = WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    );

    pool.get_or_init("sync_test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    // Not indexed in pool yet
    assert!(!pool.is_indexed("sync_test_ws").await);

    // Simulate what handle_index_command does: transition daemon.db to "ready"
    daemon_db
        .update_workspace_status("sync_test_ws", "ready")
        .unwrap();

    // sync_indexed_from_db must propagate the "ready" flag to the pool's in-memory state
    pool.sync_indexed_from_db("sync_test_ws").await;

    assert!(
        pool.is_indexed("sync_test_ws").await,
        "pool should reflect indexed=true after sync when daemon.db says ready"
    );
}

// pool.sync_indexed_from_db must be a no-op when daemon.db says "pending"
#[tokio::test]
async fn test_sync_indexed_from_db_noop_when_pending() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();

    let pool = WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    );

    pool.get_or_init("pending_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    // daemon.db status is "pending" — sync should leave pool flag as false
    pool.sync_indexed_from_db("pending_ws").await;

    assert!(
        !pool.is_indexed("pending_ws").await,
        "pool should remain not-indexed when daemon.db says pending"
    );
}

#[tokio::test]
async fn test_watcher_pool_detached_on_disconnect() {
    use crate::daemon::watcher_pool::WatcherPool;
    use std::time::Duration;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let pool = WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        None,
        Some(Arc::clone(&watcher_pool)),
        None,
    );

    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    pool.disconnect_session("test_ws").await;

    // ref_count hit 0, grace deadline should now be set
    assert!(watcher_pool.has_grace_deadline("test_ws").await);
}

#[tokio::test]
async fn test_get_does_not_block_on_unrelated_session_count_update() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db_path = tmp.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path).unwrap());
    let indexes_dir = tmp.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir).unwrap();

    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));
    let root1 = temp_workspace_root();
    pool.get_or_init("ws1", root1.path().to_path_buf())
        .await
        .expect("initial workspace should load");

    let blocking_conn = rusqlite::Connection::open(&daemon_db_path).unwrap();
    blocking_conn.execute_batch("BEGIN EXCLUSIVE;").unwrap();

    let root2 = temp_workspace_root();
    let pool_for_task = Arc::clone(&pool);
    let root2_path = root2.path().to_path_buf();
    let blocked_task =
        tokio::spawn(async move { pool_for_task.get_or_init("ws2", root2_path).await });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let existing =
        tokio::time::timeout(std::time::Duration::from_millis(200), pool.get("ws1")).await;
    assert!(
        existing.is_ok(),
        "existing workspace reads should stay available while a different workspace waits on daemon.db"
    );
    assert!(
        existing.unwrap().is_some(),
        "existing workspace should remain visible during concurrent init"
    );

    blocking_conn.execute_batch("ROLLBACK;").unwrap();
    blocked_task
        .await
        .expect("background init task should not panic")
        .expect("blocked init should complete once daemon.db lock is released");
}
