//! Tests for idle-workspace eviction in `WorkspacePool`.
//!
//! Each workspace holds Tantivy index files + SQLite connections; without
//! eviction the daemon hits EMFILE under multi-workspace eval workloads.
//! These tests cover:
//!   1. Idle workspaces are evicted after the threshold elapses.
//!   2. Recently-accessed workspaces are preserved.
//!   3. Eviction calls `SearchIndex::shutdown()` to release file locks.
//!   4. `evict_workspace()` also shuts the index down.

use std::sync::Arc;
use std::time::Duration;

use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::workspace::registry::generate_workspace_id;

fn temp_indexes_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temp indexes dir")
}

fn temp_workspace_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("Failed to create temp workspace root");
    std::fs::create_dir_all(dir.path().join(".julie")).expect("Failed to create .julie dir");
    dir
}

async fn make_workspace(pool: &Arc<WorkspacePool>, index: usize) -> (String, tempfile::TempDir) {
    let root = temp_workspace_root();
    let base_id = generate_workspace_id(root.path().to_str().expect("path is valid utf-8"))
        .expect("generate_workspace_id should succeed");
    let workspace_id = format!("{base_id}_{index}");
    pool.get_or_init(&workspace_id, root.path().to_path_buf())
        .await
        .expect("get_or_init should succeed");
    (workspace_id, root)
}

#[tokio::test]
async fn test_sweep_evicts_idle_workspace() {
    let indexes_dir = temp_indexes_dir();
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));

    let (workspace_id, _root) = make_workspace(&pool, 0).await;
    assert!(pool.get(&workspace_id).await.is_some());

    // Sleep past idle threshold (set very small for this test).
    tokio::time::sleep(Duration::from_millis(50)).await;

    let evicted = pool
        .sweep_idle_workspaces(&watcher_pool, Duration::from_millis(10))
        .await;

    assert_eq!(evicted, vec![workspace_id.clone()]);
    assert!(
        pool.get(&workspace_id).await.is_none(),
        "workspace should be removed from pool after sweep"
    );
}

#[tokio::test]
async fn test_sweep_preserves_recently_accessed_workspace() {
    let indexes_dir = temp_indexes_dir();
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));

    let (id_idle, _root_idle) = make_workspace(&pool, 0).await;
    let (id_active, _root_active) = make_workspace(&pool, 1).await;

    // Make `id_idle` age while `id_active` stays warm.
    tokio::time::sleep(Duration::from_millis(60)).await;
    // Touch `id_active` so its last_accessed is refreshed.
    let _ = pool.get(&id_active).await;

    let evicted = pool
        .sweep_idle_workspaces(&watcher_pool, Duration::from_millis(40))
        .await;

    assert!(
        evicted.contains(&id_idle),
        "idle workspace should be evicted"
    );
    assert!(
        !evicted.contains(&id_active),
        "recently-accessed workspace should be preserved"
    );
    assert!(pool.get(&id_active).await.is_some());
    assert!(pool.get(&id_idle).await.is_none());
}

#[tokio::test]
async fn test_sweep_evicts_idle_connections_for_warm_workspace() {
    let indexes_dir = temp_indexes_dir();
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));

    let (workspace_id, _root) = make_workspace(&pool, 0).await;
    let connection_pool = pool
        .connection_pool(&workspace_id)
        .await
        .expect("connection pool should exist");

    let c1 = connection_pool.acquire().await.expect("acquire c1");
    let c2 = connection_pool.acquire().await.expect("acquire c2");
    let c3 = connection_pool.acquire().await.expect("acquire c3");
    let c4 = connection_pool.acquire().await.expect("acquire c4");
    drop(c1);
    drop(c2);
    drop(c3);
    drop(c4);
    assert_eq!(
        connection_pool.stats().idle,
        4,
        "test setup should leave surplus idle pooled connections"
    );

    tokio::time::sleep(Duration::from_millis(50)).await;
    let _ = pool.get(&workspace_id).await;

    let evicted_workspaces = pool
        .sweep_idle_workspaces(&watcher_pool, Duration::from_millis(20))
        .await;

    assert!(
        evicted_workspaces.is_empty(),
        "recently touched workspace should stay loaded"
    );
    assert_eq!(
        connection_pool.stats().idle,
        2,
        "sweeper should evict stale idle DB connections down to pool minimum"
    );
}

#[tokio::test]
async fn test_sweep_shuts_down_search_index() {
    let indexes_dir = temp_indexes_dir();
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));

    let (workspace_id, _root) = make_workspace(&pool, 0).await;
    let workspace = pool.get(&workspace_id).await.expect("workspace present");
    let idx_arc = workspace
        .search_index
        .clone()
        .expect("workspace should have a search_index");
    drop(workspace);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let evicted = pool
        .sweep_idle_workspaces(&watcher_pool, Duration::from_millis(10))
        .await;
    assert_eq!(evicted, vec![workspace_id]);

    let idx = idx_arc.lock().expect("mutex should not be poisoned");
    assert!(
        idx.is_shutdown(),
        "search_index should be shut down after eviction"
    );
}

#[tokio::test]
async fn test_evict_workspace_shuts_down_search_index() {
    let indexes_dir = temp_indexes_dir();
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));

    let (workspace_id, _root) = make_workspace(&pool, 0).await;
    let workspace = pool.get(&workspace_id).await.expect("workspace present");
    let idx_arc = workspace
        .search_index
        .clone()
        .expect("workspace should have a search_index");
    drop(workspace);

    let removed = pool.evict_workspace(&workspace_id).await;
    assert!(
        removed,
        "evict_workspace should return true when entry existed"
    );

    let idx = idx_arc.lock().expect("mutex should not be poisoned");
    assert!(
        idx.is_shutdown(),
        "evict_workspace must call SearchIndex::shutdown()"
    );
}

#[tokio::test]
async fn test_get_refreshes_last_accessed() {
    let indexes_dir = temp_indexes_dir();
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));

    let (workspace_id, _root) = make_workspace(&pool, 0).await;

    // Let the entry age past the threshold.
    tokio::time::sleep(Duration::from_millis(60)).await;
    // Touch via `get()` — should reset last_accessed.
    let _ = pool.get(&workspace_id).await;

    let evicted = pool
        .sweep_idle_workspaces(&watcher_pool, Duration::from_millis(40))
        .await;

    assert!(
        !evicted.contains(&workspace_id),
        "get() must refresh last_accessed so the workspace is not evicted"
    );
}
