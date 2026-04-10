use crate::daemon::watcher_pool::WatcherPool;
use std::time::Duration;

#[tokio::test]
async fn test_watcher_pool_attach_detach_ref_count() {
    let pool = WatcherPool::new(Duration::from_secs(300));

    pool.increment_ref("ws1").await;
    assert_eq!(pool.ref_count("ws1").await, 1);

    pool.increment_ref("ws1").await;
    assert_eq!(pool.ref_count("ws1").await, 2);

    pool.decrement_ref("ws1").await;
    assert_eq!(pool.ref_count("ws1").await, 1);

    pool.decrement_ref("ws1").await;
    assert_eq!(pool.ref_count("ws1").await, 0);
    // Grace deadline should now be set
    assert!(pool.has_grace_deadline("ws1").await);
}

#[tokio::test]
async fn test_watcher_pool_reattach_cancels_grace() {
    let pool = WatcherPool::new(Duration::from_secs(300));
    pool.increment_ref("ws1").await;
    pool.decrement_ref("ws1").await;
    assert!(pool.has_grace_deadline("ws1").await);

    // Reattach should cancel the grace deadline
    pool.increment_ref("ws1").await;
    assert!(!pool.has_grace_deadline("ws1").await);
    assert_eq!(pool.ref_count("ws1").await, 1);
}

#[tokio::test]
async fn test_reaper_removes_expired_entries() {
    // Use a very short grace period for testing
    let pool = WatcherPool::new(Duration::from_millis(50));
    pool.increment_ref("ws1").await;
    pool.decrement_ref("ws1").await;
    assert!(pool.has_grace_deadline("ws1").await);

    // Wait for grace period to expire
    tokio::time::sleep(Duration::from_millis(100)).await;

    let reaped = pool.reap_expired().await;
    assert_eq!(reaped, vec!["ws1"]);
    // Entry is gone: ref_count defaults to 0, no grace deadline
    assert_eq!(pool.ref_count("ws1").await, 0);
    assert!(!pool.has_grace_deadline("ws1").await);
}

#[tokio::test]
async fn test_decrement_ref_below_zero_clamps() {
    let pool = WatcherPool::new(Duration::from_secs(300));
    pool.increment_ref("ws1").await;
    pool.decrement_ref("ws1").await;
    // Extra decrement should not underflow
    pool.decrement_ref("ws1").await;
    assert_eq!(pool.ref_count("ws1").await, 0);
}

#[tokio::test]
async fn test_reaper_leaves_entries_within_grace() {
    let pool = WatcherPool::new(Duration::from_secs(300));
    pool.increment_ref("ws1").await;
    pool.decrement_ref("ws1").await;
    // Don't wait — grace period hasn't expired yet
    let reaped = pool.reap_expired().await;
    assert!(reaped.is_empty());
    // Entry should still be there
    assert!(pool.has_grace_deadline("ws1").await);
}

/// `update_all_provider` on an empty pool should be a no-op and return 0,
/// not panic. The daemon's background init task always calls this on
/// publish_ready, even if no sessions have connected yet.
#[tokio::test]
async fn test_update_all_provider_empty_pool() {
    let pool = WatcherPool::new(Duration::from_secs(300));
    let count = pool.update_all_provider(None).await;
    assert_eq!(count, 0, "empty pool should return 0 watchers updated");
}

/// `update_all_provider` should also be a no-op when entries exist via
/// `increment_ref` (which doesn't create the inner `IncrementalIndexer`).
/// The actual file watcher only gets created on `attach`, which requires
/// a real workspace + db. The plain `increment_ref` path leaves
/// `entry.watcher == None`, so update_all_provider should skip those.
#[tokio::test]
async fn test_update_all_provider_skips_entries_without_watcher() {
    let pool = WatcherPool::new(Duration::from_secs(300));
    pool.increment_ref("ws1").await;
    pool.increment_ref("ws2").await;

    // Both entries exist but neither has an IncrementalIndexer (no attach
    // was called). update_all_provider should iterate cleanly and return 0.
    let count = pool.update_all_provider(None).await;
    assert_eq!(
        count, 0,
        "entries without an IncrementalIndexer should not be counted"
    );
}
