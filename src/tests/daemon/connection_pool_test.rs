use std::sync::Arc;
use std::time::{Duration, Instant};

use tempfile::tempdir;
use tokio::time::timeout;

use crate::daemon::connection_pool::{PoolStats, WorkspaceConnectionPool};

fn make_pool(min: usize, max: usize) -> Arc<WorkspaceConnectionPool> {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    // Keep dir alive by leaking it for the test duration — tempdir auto-cleans on drop,
    // but since the pool holds a PathBuf we need the dir to live long enough.
    // Box::leak is acceptable in test code.
    let _ = Box::leak(Box::new(dir));
    Arc::new(WorkspaceConnectionPool::with_limits(db_path, min, max).unwrap())
}

// ─────────────────────────────────────────────
// Test 1: basic connection works end-to-end
// ─────────────────────────────────────────────
#[tokio::test]
async fn test_acquire_returns_working_connection() {
    let pool = make_pool(2, 4);
    let conn = pool.acquire().await.unwrap();
    conn.execute_batch("CREATE TABLE foo (x INT); INSERT INTO foo VALUES (42);")
        .unwrap();
    let val: i64 = conn
        .query_row("SELECT x FROM foo", [], |row| row.get(0))
        .unwrap();
    assert_eq!(val, 42);
}

// ─────────────────────────────────────────────
// Test 2: PooledConn is Send — held across .await
// ─────────────────────────────────────────────
#[tokio::test]
async fn test_pooled_conn_is_send_across_await() {
    let pool = make_pool(2, 4);
    let conn = pool.acquire().await.unwrap();
    // Create table before the sleep
    conn.execute_batch("CREATE TABLE bar (v INT); INSERT INTO bar VALUES (99);")
        .unwrap();
    // Yield to the runtime — PooledConn must be Send to compile here
    tokio::time::sleep(Duration::from_millis(1)).await;
    // Use the connection after the await to prove it wasn't dropped
    let val: i64 = conn
        .query_row("SELECT v FROM bar", [], |row| row.get(0))
        .unwrap();
    assert_eq!(val, 99);
}

// ─────────────────────────────────────────────
// Test 3: third acquire blocks when max reached
// ─────────────────────────────────────────────
#[tokio::test]
async fn test_acquire_blocks_when_max_reached() {
    let pool = make_pool(1, 2);
    let c1 = pool.acquire().await.unwrap();
    let c2 = pool.acquire().await.unwrap();

    let pool2 = Arc::clone(&pool);
    let task = tokio::spawn(async move { pool2.acquire().await.unwrap() });

    // Third acquire should be blocked — 100ms timeout must expire
    let timed_out = timeout(Duration::from_millis(100), task).await;
    assert!(timed_out.is_err(), "expected third acquire to block");

    // Now drop one held connection — frees a slot, waiter should unblock
    drop(c1);

    // Give pool a moment to wake the waiter then reacquire
    let pool3 = Arc::clone(&pool);
    let task2 = tokio::spawn(async move { pool3.acquire().await.unwrap() });
    let result = timeout(Duration::from_millis(200), task2).await;
    assert!(result.is_ok(), "expected acquire to succeed after drop");

    drop(c2); // keep lints happy
}

// ─────────────────────────────────────────────
// Test 4: drop returns connection to pool
// ─────────────────────────────────────────────
#[tokio::test]
async fn test_drop_returns_connection_to_pool() {
    let pool = make_pool(1, 2);

    // After construction: min=1 pre-warmed → idle=1, in_use=0
    let PoolStats { idle, in_use, .. } = pool.stats();
    assert_eq!(idle, 1, "expected 1 idle after construction");
    assert_eq!(in_use, 0, "expected 0 in_use after construction");

    // Acquire → idle=0, in_use=1
    let conn = pool.acquire().await.unwrap();
    let PoolStats { idle, in_use, .. } = pool.stats();
    assert_eq!(idle, 0, "expected 0 idle after acquire");
    assert_eq!(in_use, 1, "expected 1 in_use after acquire");

    // Drop → idle=1, in_use=0
    drop(conn);
    let PoolStats { idle, in_use, .. } = pool.stats();
    assert_eq!(idle, 1, "expected 1 idle after drop");
    assert_eq!(in_use, 0, "expected 0 in_use after drop");
}

#[tokio::test]
async fn test_drop_rolls_back_open_transaction_before_reuse() {
    let pool = make_pool(1, 1);

    {
        let conn = pool.acquire().await.unwrap();
        conn.execute_batch(
            "CREATE TABLE tx_probe (value INTEGER);
             BEGIN DEFERRED TRANSACTION;
             INSERT INTO tx_probe VALUES (42);",
        )
        .unwrap();
        assert!(
            !conn.is_autocommit(),
            "test setup must leave an open transaction on the pooled connection"
        );
    }

    let conn = pool.acquire().await.unwrap();
    assert!(
        conn.is_autocommit(),
        "pool must not return a connection with an open transaction"
    );
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tx_probe", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        count, 0,
        "uncommitted work from a leaked transaction must be rolled back before reuse"
    );
}

// ─────────────────────────────────────────────
// Test 5: evict_idle never goes below min
// ─────────────────────────────────────────────
#[tokio::test]
async fn test_evict_idle_below_min_is_never_evicted() {
    let pool = make_pool(2, 4);
    // Acquire 4 (2 pre-warmed get consumed, 2 more opened on demand)
    let c1 = pool.acquire().await.unwrap();
    let c2 = pool.acquire().await.unwrap();
    let c3 = pool.acquire().await.unwrap();
    let c4 = pool.acquire().await.unwrap();
    // Drop all → idle=4, in_use=0
    drop(c1);
    drop(c2);
    drop(c3);
    drop(c4);
    assert_eq!(pool.stats().idle, 4);

    // Advance "now" by 120s — all 4 are older than 60s threshold
    let now = Instant::now() + Duration::from_secs(120);
    let evicted = pool.evict_idle(Duration::from_secs(60), now);
    // Should evict 4 - min(2) = 2
    assert_eq!(evicted, 2, "expected 2 evicted (down to min=2)");
    assert_eq!(pool.stats().idle, 2);
}

// ─────────────────────────────────────────────
// Test 6: evict_idle keeps recent connections
// ─────────────────────────────────────────────
#[tokio::test]
async fn test_evict_idle_keeps_recent() {
    let pool = make_pool(1, 4);
    // Pre-warm gives idle=1. Acquire 3 more → in_use=3, idle consumed then emptied.
    // Actually: acquire first uses the pre-warmed one, then opens 2 more on demand.
    let c1 = pool.acquire().await.unwrap();
    let c2 = pool.acquire().await.unwrap();
    let c3 = pool.acquire().await.unwrap();
    // in_use=3, idle=0

    // t0: record a baseline "now" that represents when the first drop happens
    let t0 = Instant::now();

    // Drop c1 at t0 → idle=1, in_use=2
    drop(c1);

    // Drop c2 at t0 as well (same instant in injected-time world) → idle=2, in_use=1
    drop(c2);

    // Pass t0 + 30s — both entries are 30s old, threshold is 60s → nothing evicted
    let evicted = pool.evict_idle(Duration::from_secs(60), t0 + Duration::from_secs(30));
    assert_eq!(evicted, 0, "both entries under threshold, expect 0 evicted");
    assert_eq!(pool.stats().idle, 2);

    // Pass t0 + 120s — both entries are 120s old, above 60s threshold.
    // idle=2, in_use=1 → (idle + in_use) = 3 > min(1), so can evict up to 2.
    // Both are stale → evict both down to max(0, min=1 - in_use=1) = 0 from idle,
    // but we must keep (idle + in_use) >= min. in_use=1 ≥ min=1, so idle can hit 0.
    let evicted2 = pool.evict_idle(Duration::from_secs(60), t0 + Duration::from_secs(120));
    // With in_use=1 already at or above min=1, both idle entries can be evicted.
    assert_eq!(
        evicted2, 2,
        "both stale entries evictable when in_use covers min"
    );
    assert_eq!(pool.stats().idle, 0);

    drop(c3);
}

// ─────────────────────────────────────────────
// Test 7: evict with nothing idle returns zero
// ─────────────────────────────────────────────
#[tokio::test]
async fn test_evict_with_no_idle_returns_zero() {
    let pool = make_pool(2, 4);
    // Drain the pre-warmed connections
    let c1 = pool.acquire().await.unwrap();
    let c2 = pool.acquire().await.unwrap();
    // idle=0, in_use=2
    assert_eq!(pool.stats().idle, 0);

    let evicted = pool.evict_idle(Duration::ZERO, Instant::now());
    assert_eq!(evicted, 0);

    drop(c1);
    drop(c2);
}

// ─────────────────────────────────────────────
// Test 8: acquire never exceeds max
// ─────────────────────────────────────────────
#[tokio::test]
async fn test_acquire_never_exceeds_max() {
    let pool = make_pool(1, 3);

    // Spawn 5 concurrent acquires
    let mut handles = Vec::new();
    for _ in 0..5 {
        let p = Arc::clone(&pool);
        handles.push(tokio::spawn(async move { p.acquire().await.unwrap() }));
    }

    // Give tokio a moment to schedule all tasks
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Pool max=3 so in_use should be exactly 3, idle=0
    let stats = pool.stats();
    assert_eq!(stats.in_use, 3, "at most max=3 connections in-use");
    assert_eq!(stats.idle, 0);

    // The pool is saturated; a 6th acquire with a short timeout should fail
    let p = Arc::clone(&pool);
    let sixth = tokio::spawn(async move { p.acquire().await.unwrap() });
    let timed_out = timeout(Duration::from_millis(50), sixth).await;
    assert!(
        timed_out.is_err(),
        "6th acquire must block with max=3 all in-use"
    );

    // Drop all 5 tasks by letting them be cancelled (they still hold the guards in handles).
    // Actually the handles haven't returned yet — the 3 that got connections are waiting
    // in the spawn'd tasks, and the 2 that are blocked are also waiting.
    // We need to drive the pool to completion by dropping guards.
    // Abort the pending tasks and join the 3 that completed.
    for h in handles {
        h.abort();
    }
}
