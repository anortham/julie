//! Tests that SymbolDatabase wrapping a PooledConn behaves identically to one
//! wrapping an owned Connection.

use std::sync::Arc;
use std::time::Duration;

use tempfile::tempdir;
use tokio::sync::Barrier;
use tokio::time::timeout;

use crate::daemon::connection_pool::WorkspaceConnectionPool;
use crate::database::SymbolDatabase;

#[tokio::test]
async fn test_pooled_symbol_database_round_trips_through_owned_schema() {
    // Step 1: open an OWNED SymbolDatabase to run migrations + create schema.
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let owned_db = SymbolDatabase::new(&db_path).expect("owned db init");
    drop(owned_db); // release the file lock so the pool can open

    // Step 2: open the same file via WorkspaceConnectionPool, wrap in pooled
    // SymbolDatabase, and verify a known schema query works.
    let pool = Arc::new(WorkspaceConnectionPool::with_limits(db_path.clone(), 1, 2).unwrap());
    let pooled = pool.acquire().await.unwrap();
    let pooled_db = SymbolDatabase::from_pooled(pooled, db_path);

    // Ask for a count of symbols — must be 0 on a fresh schema.
    let count = pooled_db
        .count_symbols_for_workspace()
        .expect("pooled db count_symbols_for_workspace should work");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_pooled_symbol_database_drop_returns_connection() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let owned = SymbolDatabase::new(&db_path).expect("owned init");
    drop(owned);

    let pool = Arc::new(WorkspaceConnectionPool::with_limits(db_path.clone(), 1, 2).unwrap());
    let initial_stats = pool.stats();

    {
        let pooled = pool.acquire().await.unwrap();
        let _db = SymbolDatabase::from_pooled(pooled, db_path);
        let mid_stats = pool.stats();
        assert!(
            mid_stats.in_use > initial_stats.in_use,
            "expected in_use to grow while pooled db is alive"
        );
    }

    // After drop, the pooled connection should be returned.
    let final_stats = pool.stats();
    assert_eq!(
        final_stats.in_use,
        initial_stats.in_use,
        "drop of pooled SymbolDatabase must return conn"
    );
}

/// A2.3 regression net: concurrent readers acquired through the pool must not
/// serialize. If a future change re-introduces a global mutex around the
/// SymbolDatabase, this test will deadlock at the barrier (or fail the
/// peak-in-use assertion).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_pooled_symbol_database_supports_concurrent_readers() {
    const N: usize = 4;

    let dir = tempdir().unwrap();
    let db_path = dir.path().join("concurrent.db");
    // Initialize the schema on disk through an owned DB, then drop it so the
    // pool can open fresh connections.
    let owned = SymbolDatabase::new(&db_path).expect("owned init");
    drop(owned);

    let pool =
        Arc::new(WorkspaceConnectionPool::with_limits(db_path.clone(), 1, N).expect("pool init"));

    // Barrier ensures every task holds its connection at the same moment.
    // A single shared Mutex would deadlock here (N-1 tasks could not acquire
    // until the first released, but no task releases until all reach the
    // barrier).
    let barrier = Arc::new(Barrier::new(N));

    let mut handles = Vec::with_capacity(N);
    for _ in 0..N {
        let pool = Arc::clone(&pool);
        let db_path = db_path.clone();
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            let pooled = pool.acquire().await.expect("acquire");
            let db = SymbolDatabase::from_pooled(pooled, db_path);
            // Perform a real read to prove the connection works.
            let count = db
                .count_symbols_for_workspace()
                .expect("count_symbols_for_workspace");
            // Wait at the barrier while holding the connection.
            barrier.wait().await;
            count
        }));
    }

    // If pool concurrency is broken, the barrier never releases and we time out.
    let results = timeout(Duration::from_secs(5), async {
        let mut counts = Vec::with_capacity(N);
        for h in handles {
            counts.push(h.await.expect("task panicked"));
        }
        counts
    })
    .await
    .expect("concurrent readers must not deadlock — pool serialization regression?");

    assert_eq!(results.len(), N, "all readers should complete");
    assert!(
        results.iter().all(|c| *c == 0),
        "each pooled reader should see an empty schema"
    );

    // All connections were dropped by now; pool should be idle.
    let stats = pool.stats();
    assert_eq!(
        stats.in_use, 0,
        "after readers complete, all connections must return to the pool"
    );
    assert!(
        stats.idle >= 1,
        "pool should retain at least the min (1) idle connection"
    );
}
