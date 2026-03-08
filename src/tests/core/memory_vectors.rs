//! Tests for memory embedding vector storage (migration 012 + CRUD + KNN).
//!
//! Mirrors `vector_storage.rs` pattern but for the `memory_vectors` table.

#[cfg(test)]
mod tests {
    use crate::database::{SymbolDatabase, LATEST_SCHEMA_VERSION};
    use tempfile::TempDir;

    fn create_test_db() -> (SymbolDatabase, TempDir) {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("Failed to create database");
        (db, dir)
    }

    // ── Migration 012 ──────────────────────────────────────────────────

    #[test]
    fn test_migration_012_creates_memory_vectors_table() {
        let (db, _dir) = create_test_db();

        let table_exists: bool = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memory_vectors'",
                [],
                |row| row.get::<_, i32>(0).map(|c| c > 0),
            )
            .unwrap();

        assert!(
            table_exists,
            "memory_vectors table should exist after migration 012"
        );
    }

    #[test]
    fn test_migration_012_is_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create database (runs all migrations including 012)
        {
            let _db = SymbolDatabase::new(&db_path).unwrap();
        }

        // Re-open — should not error
        let db = SymbolDatabase::new(&db_path).unwrap();
        let version = db.get_schema_version().unwrap();
        assert_eq!(version, LATEST_SCHEMA_VERSION);
    }

    // ── Memory Embedding CRUD ──────────────────────────────────────────

    #[test]
    fn test_store_memory_embeddings() {
        let (mut db, _dir) = create_test_db();

        let embeddings = vec![
            ("mem-001".to_string(), vec![0.1_f32; 384]),
            ("mem-002".to_string(), vec![0.2_f32; 384]),
        ];

        let count = db.store_memory_embeddings(&embeddings).unwrap();
        assert_eq!(count, 2);
        assert_eq!(db.memory_embedding_count().unwrap(), 2);
    }

    #[test]
    fn test_delete_memory_embedding() {
        let (mut db, _dir) = create_test_db();

        let embeddings = vec![
            ("mem-001".to_string(), vec![0.1_f32; 384]),
            ("mem-002".to_string(), vec![0.2_f32; 384]),
        ];
        db.store_memory_embeddings(&embeddings).unwrap();

        let deleted = db.delete_memory_embedding("mem-001").unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(db.memory_embedding_count().unwrap(), 1);
    }

    #[test]
    fn test_delete_nonexistent_memory_embedding() {
        let (mut db, _dir) = create_test_db();

        let deleted = db.delete_memory_embedding("does-not-exist").unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_clear_all_memory_embeddings() {
        let (mut db, _dir) = create_test_db();

        let embeddings = vec![
            ("mem-001".to_string(), vec![0.1_f32; 384]),
            ("mem-002".to_string(), vec![0.2_f32; 384]),
            ("mem-003".to_string(), vec![0.3_f32; 384]),
        ];
        db.store_memory_embeddings(&embeddings).unwrap();
        assert_eq!(db.memory_embedding_count().unwrap(), 3);

        db.clear_all_memory_embeddings().unwrap();
        assert_eq!(db.memory_embedding_count().unwrap(), 0);
    }

    // ── KNN Search ─────────────────────────────────────────────────────

    #[test]
    fn test_knn_memory_search_returns_nearest() {
        let (mut db, _dir) = create_test_db();

        // Store 3 embeddings with known distances from query
        let mut vec_close = vec![0.0_f32; 384];
        vec_close[0] = 1.0; // will be closest to query

        let mut vec_mid = vec![0.0_f32; 384];
        vec_mid[0] = 0.5;
        vec_mid[1] = 0.5;

        let mut vec_far = vec![0.0_f32; 384];
        vec_far[1] = 1.0; // furthest from query

        let embeddings = vec![
            ("mem-close".to_string(), vec_close),
            ("mem-mid".to_string(), vec_mid),
            ("mem-far".to_string(), vec_far),
        ];
        db.store_memory_embeddings(&embeddings).unwrap();

        // Query: same direction as vec_close
        let mut query = vec![0.0_f32; 384];
        query[0] = 1.0;

        let results = db.knn_memory_search(&query, 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "mem-close", "closest should be first");
        assert_eq!(results[1].0, "mem-mid", "mid-distance should be second");
    }

    #[test]
    fn test_knn_memory_search_empty_table() {
        let (db, _dir) = create_test_db();

        let query = vec![0.1_f32; 384];
        let results = db.knn_memory_search(&query, 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_knn_memory_search_respects_limit() {
        let (mut db, _dir) = create_test_db();

        let embeddings: Vec<_> = (0..10)
            .map(|i| {
                let mut v = vec![0.0_f32; 384];
                v[0] = i as f32 / 10.0;
                (format!("mem-{:03}", i), v)
            })
            .collect();
        db.store_memory_embeddings(&embeddings).unwrap();

        let query = vec![0.5_f32; 384];
        let results = db.knn_memory_search(&query, 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    // ── Dynamic Dimensions ─────────────────────────────────────────────

    #[test]
    fn test_recreate_memory_vectors_table_changes_dimensions() {
        let (mut db, _dir) = create_test_db();

        // Store with default 384 dims
        let embeddings = vec![("mem-001".to_string(), vec![0.1_f32; 384])];
        db.store_memory_embeddings(&embeddings).unwrap();
        assert_eq!(db.memory_embedding_count().unwrap(), 1);

        // Recreate with 768 dims — should clear existing data
        db.recreate_memory_vectors_table(768).unwrap();
        assert_eq!(db.memory_embedding_count().unwrap(), 0);

        // Should accept 768-dim vectors now
        let big_embeddings = vec![("mem-002".to_string(), vec![0.2_f32; 768])];
        let count = db.store_memory_embeddings(&big_embeddings).unwrap();
        assert_eq!(count, 1);
    }
}
