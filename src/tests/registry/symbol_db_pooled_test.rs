//! Tests that SymbolDatabase wrapping a PooledConn behaves identically to one
//! wrapping an owned Connection.

use std::sync::Arc;
use std::time::Duration;

use tempfile::tempdir;
use tokio::sync::Barrier;
use tokio::time::timeout;

use crate::database::SymbolDatabase;
use crate::registry::connection_pool::WorkspaceConnectionPool;

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
        final_stats.in_use, initial_stats.in_use,
        "drop of pooled SymbolDatabase must return conn"
    );
}

#[tokio::test]
async fn test_pooled_symbol_database_read_snapshot_rolls_back_on_drop() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("snapshot.db");
    let owned = SymbolDatabase::new(&db_path).expect("owned init");
    drop(owned);

    let pool = Arc::new(WorkspaceConnectionPool::with_limits(db_path.clone(), 1, 1).unwrap());

    {
        let pooled = pool.acquire().await.unwrap();
        let db = SymbolDatabase::from_pooled(pooled, db_path.clone());
        let snapshot = db.into_read_snapshot().expect("begin read snapshot");
        assert!(
            !snapshot.is_autocommit_for_test(),
            "read snapshot must hold a transaction while alive"
        );
        assert_eq!(
            snapshot
                .count_symbols_for_workspace()
                .expect("read inside snapshot"),
            0
        );
    }

    let pooled = pool.acquire().await.unwrap();
    assert!(
        pooled.is_autocommit(),
        "dropping the read snapshot must return an autocommit connection to the pool"
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

/// A2.2c-codex-follow-up regression net: `JulieWorkspace::request_db` must NOT
/// lock the legacy `Arc<Mutex<SymbolDatabase>>` (`workspace.db`) when acquiring
/// a pooled connection.
///
/// **The bug Codex caught:** `request_db` originally cloned `file_path` by
/// locking `self.db` before calling `pool.acquire()`. Watcher / bulk-indexer /
/// any legacy write path holds that same mutex, so pooled readers (including
/// health snapshots) serialized behind those writers — defeating the whole
/// purpose of pooling.
///
/// **The fix:** `request_db` now reads `file_path` from `pool.db_path()` and
/// never touches `workspace.db`. This test holds `workspace.db.lock()` and
/// asserts `request_db` still completes under a tight timeout.
///
/// If the regression returns (e.g., someone adds a `self.db.lock()` back into
/// `request_db`), the spawned task will block on the lock and the outer
/// `timeout(500ms)` will fail the test.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_request_db_does_not_block_on_legacy_workspace_mutex() {
    use crate::workspace::JulieWorkspace;

    let dir = tempdir().unwrap();
    let workspace_root = dir.path().to_path_buf();

    // Step 1: build a real JulieWorkspace; this initializes `workspace.db`
    // (the legacy Arc<Mutex<SymbolDatabase>>) under `<root>/.julie/db/`.
    let workspace = Arc::new(
        JulieWorkspace::initialize(workspace_root.clone())
            .await
            .expect("workspace init"),
    );
    let legacy_db = workspace
        .db
        .as_ref()
        .expect("initialize must populate workspace.db")
        .clone();

    // Extract the canonical db file path from the initialized SymbolDatabase.
    // We can't use `workspace.db_path()` (that's the legacy stdio path) — the
    // real path lives on the initialized DB (under workspace_db_path which
    // includes the workspace_id). Production daemon code does the same one-
    // time extraction inside WorkspacePool::init_workspace_locked.
    let canonical_db_path = {
        let guard = legacy_db.lock().expect("legacy db lock for setup");
        guard.file_path.clone()
    };

    // Step 2: build the connection pool over the SAME db file the workspace
    // initialized. Pre-warming min=1 ensures `acquire()` is instant.
    let pool = Arc::new(
        WorkspaceConnectionPool::with_limits(canonical_db_path, 1, 2)
            .expect("pool init over real workspace db"),
    );

    // Step 3: simulate a writer holding the legacy mutex. Pre-fix,
    // `request_db` would call `self.db.lock()` to clone file_path and would
    // block here until we release.
    let writer_guard = legacy_db.lock().expect("hold legacy writer mutex");

    // Step 4: call request_db under a tight timeout. The spawned task gets a
    // clone of the pool Arc; if request_db were to lock the workspace mutex
    // it would deadlock against `writer_guard`.
    let pool_for_task = Arc::clone(&pool);
    let pooled_db = timeout(Duration::from_millis(500), async move {
        pool_for_task
            .request_db()
            .await
            .expect("request_db should succeed")
    })
    .await
    .expect(
        "request_db must NOT lock workspace.db — \
         the pool was supposed to free us from that contention",
    );

    // Sanity-check the pooled DB actually works while the legacy mutex is
    // still held. Counting symbols on a fresh schema returns 0.
    let count = pooled_db
        .count_symbols_for_workspace()
        .expect("count via pooled conn");
    assert_eq!(count, 0, "fresh workspace schema is empty");

    // Step 5: release the legacy mutex; clean drop ordering.
    drop(writer_guard);
}
