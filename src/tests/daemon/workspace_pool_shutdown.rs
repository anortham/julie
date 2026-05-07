use std::sync::Arc;

use crate::daemon::workspace_pool::WorkspacePool;
use crate::workspace::registry::generate_workspace_id;

fn temp_indexes_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("Failed to create temp indexes dir")
}

fn temp_workspace_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("Failed to create temp workspace root");
    std::fs::create_dir_all(dir.path().join(".julie"))
        .expect("Failed to create .julie dir");
    dir
}

/// Set up a WorkspacePool with `n` pre-initialized workspaces.
///
/// Returns (pool, workspace_ids, workspace_roots). Roots must stay alive
/// so the temp directories are not cleaned up while the pool holds them.
async fn pool_with_workspaces(
    n: usize,
) -> (
    Arc<WorkspacePool>,
    Vec<String>,
    Vec<tempfile::TempDir>,
    tempfile::TempDir,
) {
    let indexes_dir = temp_indexes_dir();
    let pool = Arc::new(WorkspacePool::new(indexes_dir.path().to_path_buf(), None));
    let mut roots = Vec::with_capacity(n);
    let mut ids = Vec::with_capacity(n);

    for i in 0..n {
        let root = temp_workspace_root();
        let base_id = generate_workspace_id(
            root.path().to_str().expect("path is valid utf-8"),
        )
        .expect("generate_workspace_id should succeed");
        // Append index suffix to guarantee uniqueness when multiple roots
        // happen to produce the same hash (same tmpdir prefix).
        let workspace_id = format!("{base_id}_{i}");
        pool.get_or_init(&workspace_id, root.path().to_path_buf())
            .await
            .expect("get_or_init should succeed");
        ids.push(workspace_id);
        roots.push(root);
    }

    (pool, ids, roots, indexes_dir)
}

#[tokio::test]
async fn test_workspace_pool_shutdown_calls_search_index_shutdown() {
    let (pool, ids, _roots, _indexes_dir) = pool_with_workspaces(3).await;

    // Collect search_index Arcs via the public `get()` API before shutdown.
    let mut indexes: Vec<Arc<std::sync::Mutex<crate::search::SearchIndex>>> = Vec::new();
    for id in &ids {
        let ws = pool
            .get(id)
            .await
            .expect("workspace should be present before shutdown");
        let idx_arc = ws
            .search_index
            .clone()
            .expect("workspace should have a search_index");
        indexes.push(idx_arc);
    }
    assert_eq!(indexes.len(), 3, "all 3 workspaces should have a search_index");

    pool.shutdown().await;

    // After shutdown every search_index must report is_shutdown() == true.
    for (i, index_arc) in indexes.iter().enumerate() {
        let idx = index_arc.lock().expect("mutex should not be poisoned");
        assert!(
            idx.is_shutdown(),
            "workspace {i}: search_index should be shut down after pool.shutdown()"
        );
    }
}

#[tokio::test]
async fn test_workspace_pool_shutdown_recovers_from_poisoned_mutex() {
    let (pool, ids, _roots, _indexes_dir) = pool_with_workspaces(2).await;

    // Grab search_index Arcs for both workspaces before we poison one.
    let mut all_indexes: Vec<Arc<std::sync::Mutex<crate::search::SearchIndex>>> = Vec::new();
    for id in &ids {
        let ws = pool
            .get(id)
            .await
            .expect("workspace should be present before shutdown");
        let idx_arc = ws
            .search_index
            .clone()
            .expect("workspace should have a search_index");
        all_indexes.push(idx_arc);
    }
    assert_eq!(all_indexes.len(), 2, "both workspaces should have a search_index");

    // Poison the first mutex by panicking while holding its guard.
    let poisoned_arc = Arc::clone(&all_indexes[0]);
    let _ = std::panic::catch_unwind(|| {
        let _guard = poisoned_arc.lock().unwrap();
        panic!("deliberately poisoning the mutex");
    });
    assert!(
        all_indexes[0].lock().is_err(),
        "mutex should be poisoned after the panic"
    );

    // shutdown() must not panic even with a poisoned mutex.
    pool.shutdown().await;

    // Both workspaces must be shut down: the recovered-poisoned one and the healthy one.
    for (i, index_arc) in all_indexes.iter().enumerate() {
        let idx = index_arc.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            idx.is_shutdown(),
            "workspace {i}: search_index should be shut down (poisoned mutex recovered)"
        );
    }
}

/// Codex finding #3 (medium): if a search_index mutex is held by another task
/// (e.g., a hung in-flight request), `WorkspacePool::shutdown` must not block
/// indefinitely. The per-workspace timeout (2s) bounds the total shutdown
/// time, even though the held workspace cannot be shut down cleanly.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_workspace_pool_shutdown_bounded_when_mutex_is_held() {
    let (pool, ids, _roots, _indexes_dir) = pool_with_workspaces(2).await;

    // Grab the first workspace's search_index Arc before we start the test —
    // we'll hold its mutex for the duration of shutdown to simulate a hung
    // in-flight request.
    let first_idx = {
        let workspace = pool
            .get_or_init(&ids[0], _roots[0].path().to_path_buf())
            .await
            .expect("get workspace");
        workspace.search_index.clone().expect("search_index exists")
    };

    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let blocker_idx = Arc::clone(&first_idx);
    let blocker = std::thread::spawn(move || {
        let _guard = blocker_idx.lock().expect("acquire mutex");
        // Hold the lock until the test signals release.
        let _ = release_rx.recv();
    });

    // Give the blocker a beat to acquire the mutex before shutdown starts.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // shutdown() should complete within a bounded time even though one
    // workspace's mutex is held. The per-workspace timeout is 2s, so two
    // workspaces with one held should finish in well under 5s.
    let started = std::time::Instant::now();
    let timeout_duration = std::time::Duration::from_secs(5);
    let result = tokio::time::timeout(timeout_duration, pool.shutdown()).await;
    let elapsed = started.elapsed();

    // Release the held mutex so the blocker thread can exit cleanly.
    let _ = release_tx.send(());
    let _ = blocker.join();

    assert!(
        result.is_ok(),
        "pool.shutdown() must complete within {:?}; took {:?}",
        timeout_duration,
        elapsed
    );
    assert!(
        elapsed < timeout_duration,
        "shutdown must return well before the test timeout (took {:?})",
        elapsed
    );
}
