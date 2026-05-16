//! Tests that SymbolDatabase wrapping a PooledConn behaves identically to one
//! wrapping an owned Connection.

use std::sync::Arc;

use tempfile::tempdir;

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
