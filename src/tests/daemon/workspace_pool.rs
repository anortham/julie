use std::sync::Arc;
use std::sync::RwLock as StdRwLock;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::daemon::workspace_session_attachment::WorkspaceSessionAttachment;
use crate::handler::session_workspace::SessionWorkspaceState;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::WorkspaceStartupHint;

fn temp_indexes_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temp dir")
}

fn temp_workspace_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    // Create .julie directory structure so JulieWorkspace::initialize succeeds
    std::fs::create_dir_all(dir.path().join(".julie")).expect("Failed to create .julie dir");
    dir
}

fn session_attachment(
    pool: Arc<WorkspacePool>,
    watcher_pool: Option<Arc<crate::daemon::watcher_pool::WatcherPool>>,
    workspace_root: std::path::PathBuf,
) -> WorkspaceSessionAttachment {
    let session_workspace = Arc::new(StdRwLock::new(SessionWorkspaceState::new(
        WorkspaceStartupHint {
            path: workspace_root,
            source: None,
        },
    )));
    WorkspaceSessionAttachment::new(Some(pool), None, watcher_pool, None, session_workspace)
}

#[tokio::test]
async fn test_get_or_init_creates_workspace_on_first_call() {
    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None);

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None);

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None);

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
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None);

    // Initialize workspace
    pool.get_or_init("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");

    // Now get should return Some
    let ws = pool.get("test_ws").await;
    assert!(ws.is_some(), "get should return Some after init");
}

#[tokio::test]
async fn test_get_or_init_keeps_distinct_ids_and_reuses_existing_workspace() {
    let indexes_dir = temp_indexes_dir();
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None);

    assert!(
        pool.get("ws1").await.is_none(),
        "pool should start without ws1"
    );

    let root1 = temp_workspace_root();
    let ws1 = pool
        .get_or_init("ws1", root1.path().to_path_buf())
        .await
        .expect("first init should succeed");

    let root2 = temp_workspace_root();
    let ws2 = pool
        .get_or_init("ws2", root2.path().to_path_buf())
        .await
        .expect("second init should succeed");

    assert!(
        !Arc::ptr_eq(&ws1, &ws2),
        "different workspace ids should produce different workspace instances"
    );

    let ws1_again = pool
        .get_or_init("ws1", root1.path().to_path_buf())
        .await
        .expect("re-init should succeed");
    assert!(
        Arc::ptr_eq(&ws1, &ws1_again),
        "re-init of an existing workspace id should reuse the cached workspace"
    );
}

#[tokio::test]
async fn test_concurrent_get_or_init_different_workspaces() {
    let indexes_dir = temp_indexes_dir();
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));

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

    let cached_a = pool.get("ws_a").await.expect("ws_a should be cached");
    let cached_b = pool.get("ws_b").await.expect("ws_b should be cached");
    assert!(Arc::ptr_eq(&ws_a, &cached_a));
    assert!(Arc::ptr_eq(&ws_b, &cached_b));
}

#[tokio::test]
async fn test_workspace_pool_accepts_daemon_db() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());
    let indexes_dir = tmp.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir).unwrap();

    // Constructor must accept daemon_db and preserve pool configuration.
    let pool = WorkspacePool::new(indexes_dir.clone(), Some(daemon_db.clone()));
    assert_eq!(pool.indexes_dir(), indexes_dir.as_path());
    assert!(pool.get("missing").await.is_none());
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

    let pool = WorkspacePool::new(indexes_dir.clone(), Some(Arc::clone(&daemon_db)));

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
async fn test_session_attachment_increments_watcher_ref() {
    use crate::daemon::watcher_pool::WatcherPool;
    use std::time::Duration;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let pool = WorkspacePool::new(indexes_dir.path().to_path_buf(), None);

    let attachment = session_attachment(
        Arc::new(pool),
        Some(Arc::clone(&watcher_pool)),
        workspace_root.path().to_path_buf(),
    );

    attachment
        .attach_workspace_once("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("attach should initialize runtime and watcher");

    assert_eq!(watcher_pool.ref_count("test_ws").await, 1);
}

#[tokio::test]
async fn test_workspace_pool_get_or_init_does_not_attach_session_side_effects() {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::watcher_pool::WatcherPool;
    use std::time::Duration;

    let tmp = tempfile::TempDir::new().unwrap();
    let indexes_dir = tmp.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir).unwrap();
    let workspace_root = temp_workspace_root();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let pool = WorkspacePool::new(indexes_dir, Some(Arc::clone(&daemon_db)));

    pool.get_or_init("runtime_only_ws", workspace_root.path().to_path_buf())
        .await
        .expect("runtime lookup should initialize workspace");

    assert_eq!(
        daemon_db
            .get_workspace("runtime_only_ws")
            .unwrap()
            .expect("workspace row should exist")
            .session_count,
        0,
        "runtime lookup should not increment session count"
    );
    assert_eq!(
        watcher_pool.ref_count("runtime_only_ws").await,
        0,
        "runtime lookup should not attach watcher refs"
    );
}

#[tokio::test]
async fn test_watcher_pool_reuses_session_refs_and_starts_grace_on_last_disconnect() {
    use crate::daemon::watcher_pool::WatcherPool;
    use std::time::Duration;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));

    let first_session = session_attachment(
        Arc::clone(&pool),
        Some(Arc::clone(&watcher_pool)),
        workspace_root.path().to_path_buf(),
    );
    let second_session = session_attachment(
        Arc::clone(&pool),
        Some(Arc::clone(&watcher_pool)),
        workspace_root.path().to_path_buf(),
    );

    first_session
        .attach_workspace_once("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("first session attach should succeed");
    second_session
        .attach_workspace_once("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("second session attach should reuse the runtime");

    assert_eq!(
        watcher_pool.ref_count("test_ws").await,
        2,
        "shared sessions should increment the same watcher ref count"
    );
    assert!(
        !watcher_pool.has_grace_deadline("test_ws").await,
        "grace should stay off while at least one session is attached"
    );

    first_session
        .detach_workspace_once("test_ws")
        .await
        .expect("first session detach should succeed");
    assert_eq!(
        watcher_pool.ref_count("test_ws").await,
        1,
        "disconnecting one session should keep the shared watcher alive"
    );
    assert!(
        !watcher_pool.has_grace_deadline("test_ws").await,
        "grace should not start until the last session disconnects"
    );

    second_session
        .detach_workspace_once("test_ws")
        .await
        .expect("second session detach should succeed");
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

    let pool = WorkspacePool::new(indexes_dir, Some(Arc::clone(&daemon_db)));

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

#[tokio::test]
async fn test_get_or_init_rejects_missing_workspace_without_creating_path_or_row() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());
    let indexes_dir = tmp.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir).unwrap();

    let missing_root = tmp.path().join("missing-workspace");
    let workspace_id = "missing_ws";
    let pool = WorkspacePool::new(indexes_dir, Some(Arc::clone(&daemon_db)));

    let result = pool.get_or_init(workspace_id, missing_root.clone()).await;

    assert!(
        result.is_err(),
        "missing workspace roots must be rejected before initialization"
    );
    assert!(
        !missing_root.exists(),
        "get_or_init must not create a missing workspace root"
    );
    assert!(
        daemon_db.get_workspace(workspace_id).unwrap().is_none(),
        "failed initialization must not leave a daemon registry row"
    );
}

#[tokio::test]
async fn test_get_or_init_rejects_sensitive_root_without_registering_row() {
    let tmp = tempfile::TempDir::new().unwrap();
    let daemon_db = Arc::new(DaemonDatabase::open(&tmp.path().join("daemon.db")).unwrap());
    let indexes_dir = tmp.path().join("indexes");
    std::fs::create_dir_all(&indexes_dir).unwrap();

    let workspace_id = "sensitive_root_ws";
    let pool = WorkspacePool::new(indexes_dir, Some(Arc::clone(&daemon_db)));

    let result = pool
        .get_or_init(workspace_id, std::path::PathBuf::from("/"))
        .await;

    assert!(
        result.is_err(),
        "sensitive filesystem roots must be rejected before initialization"
    );
    assert!(
        daemon_db.get_workspace(workspace_id).unwrap().is_none(),
        "sensitive root rejection must not leave a daemon registry row"
    );
}

#[tokio::test]
async fn test_session_attachment_detach_starts_watcher_grace() {
    use crate::daemon::watcher_pool::WatcherPool;
    use std::time::Duration;

    let indexes_dir = temp_indexes_dir();
    let workspace_root = temp_workspace_root();
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));
    let attachment = session_attachment(
        Arc::clone(&pool),
        Some(Arc::clone(&watcher_pool)),
        workspace_root.path().to_path_buf(),
    );

    attachment
        .attach_workspace_once("test_ws", workspace_root.path().to_path_buf())
        .await
        .expect("attach should initialize runtime and watcher");

    attachment
        .detach_workspace_once("test_ws")
        .await
        .expect("detach should release watcher");

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
