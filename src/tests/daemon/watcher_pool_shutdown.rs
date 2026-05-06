use crate::daemon::watcher_pool::WatcherPool;
use crate::tools::workspace::indexing::state::IndexingRuntimeState;
use crate::workspace::{JulieWorkspace, WorkspaceConfig};
use std::path::PathBuf;
use std::time::Duration;

fn workspace_without_watcher(root: impl Into<PathBuf>) -> JulieWorkspace {
    let root = root.into();
    JulieWorkspace {
        julie_dir: root.join(".julie"),
        root,
        db: None,
        search_index: None,
        watcher: None,
        embedding_provider: None,
        embedding_runtime_status: None,
        config: WorkspaceConfig::default(),
        index_root_override: None,
        indexing_runtime: IndexingRuntimeState::shared(),
    }
}

async fn attach_without_watcher(pool: &WatcherPool, workspace_id: &str) {
    let workspace = workspace_without_watcher(std::env::temp_dir().join(workspace_id));
    pool.attach(workspace_id, &workspace, None)
        .await
        .expect("attach without db/search should still update ref count");
}

/// After `shutdown()`, all entries are removed from the pool regardless of
/// their ref_count or grace state. No `IncrementalIndexer` task remains live.
#[tokio::test]
async fn test_watcher_pool_shutdown_drops_all_watchers() {
    let pool = WatcherPool::new(Duration::from_secs(300));

    // Attach N=3 workspaces, each with ref_count >= 1 (no grace deadline yet).
    attach_without_watcher(&pool, "ws-alpha").await;
    attach_without_watcher(&pool, "ws-beta").await;
    attach_without_watcher(&pool, "ws-beta").await; // second session on same workspace
    attach_without_watcher(&pool, "ws-gamma").await;

    // Confirm all three workspaces are tracked before shutdown.
    assert_eq!(pool.ref_count("ws-alpha").await, 1);
    assert_eq!(pool.ref_count("ws-beta").await, 2);
    assert_eq!(pool.ref_count("ws-gamma").await, 1);

    // Shutdown must remove all entries unconditionally.
    pool.shutdown().await;

    // After shutdown the entries map must be empty.
    assert_eq!(
        pool.ref_count("ws-alpha").await,
        0,
        "ws-alpha should be gone after shutdown"
    );
    assert_eq!(
        pool.ref_count("ws-beta").await,
        0,
        "ws-beta should be gone after shutdown"
    );
    assert_eq!(
        pool.ref_count("ws-gamma").await,
        0,
        "ws-gamma should be gone after shutdown"
    );
    // ref_count returns 0 for missing keys, so also verify no grace deadline
    // exists (which would only be set on an entry that still exists).
    assert!(
        !pool.has_grace_deadline("ws-alpha").await,
        "no grace deadline should exist after shutdown"
    );
    assert!(
        !pool.has_grace_deadline("ws-beta").await,
        "no grace deadline should exist after shutdown"
    );
    assert!(
        !pool.has_grace_deadline("ws-gamma").await,
        "no grace deadline should exist after shutdown"
    );
}

/// `shutdown()` on an already-empty pool is a no-op (no panic).
#[tokio::test]
async fn test_watcher_pool_shutdown_empty_pool_is_noop() {
    let pool = WatcherPool::new(Duration::from_secs(300));
    // Should not panic.
    pool.shutdown().await;
}

/// `shutdown()` handles a mix of states: some entries have a grace deadline
/// (ref_count == 0), others are still active (ref_count > 0). All must be
/// removed.
#[tokio::test]
async fn test_watcher_pool_shutdown_mixed_states() {
    let pool = WatcherPool::new(Duration::from_secs(300));

    // ws1: active (ref_count=1)
    attach_without_watcher(&pool, "ws1").await;

    // ws2: attached then detached — ref_count=0, grace deadline set
    attach_without_watcher(&pool, "ws2").await;
    pool.detach("ws2").await;
    assert!(pool.has_grace_deadline("ws2").await);

    pool.shutdown().await;

    assert_eq!(pool.ref_count("ws1").await, 0);
    assert_eq!(pool.ref_count("ws2").await, 0);
    assert!(!pool.has_grace_deadline("ws1").await);
    assert!(!pool.has_grace_deadline("ws2").await);
}
