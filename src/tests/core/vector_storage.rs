//! Tests for sqlite-vec vector storage (database::vectors).

#[cfg(test)]
mod tests {
    use crate::database::SymbolDatabase;
    use tempfile::TempDir;

    /// Helper: create a fresh SymbolDatabase in a temp directory.
    fn create_test_db() -> (SymbolDatabase, TempDir) {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("Failed to create database");
        (db, dir)
    }

    /// Helper: insert a symbol into the symbols table so we can join on it.
    fn insert_test_symbol(db: &mut SymbolDatabase, id: &str, name: &str, file_path: &str) {
        insert_test_symbol_with_lang(db, id, name, file_path, "rust");
    }

    /// Helper: insert a symbol with a specific language.
    fn insert_test_symbol_with_lang(
        db: &mut SymbolDatabase,
        id: &str,
        name: &str,
        file_path: &str,
        language: &str,
    ) {
        // File record must exist first (foreign key constraint)
        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES (?, ?, 'deadbeef', 100, 0, 0)",
                rusqlite::params![file_path, language],
            )
            .expect("Failed to insert test file");
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, file_path, language, start_line, end_line, reference_score)
                 VALUES (?, ?, 'function', ?, ?, 1, 10, 0.0)",
                rusqlite::params![id, name, file_path, language],
            )
            .expect("Failed to insert test symbol");
    }

    #[test]
    fn test_store_and_count_embeddings() {
        let (mut db, _dir) = create_test_db();
        insert_test_symbol(&mut db, "sym1", "process_data", "src/lib.rs");
        insert_test_symbol(&mut db, "sym2", "handle_error", "src/lib.rs");

        let embeddings = vec![
            ("sym1".to_string(), vec![0.1_f32; 384]),
            ("sym2".to_string(), vec![0.2_f32; 384]),
        ];

        let stored = db.store_embeddings(&embeddings).unwrap();
        assert_eq!(stored, 2);
        assert_eq!(db.embedding_count().unwrap(), 2);
    }

    #[test]
    fn test_store_empty_batch() {
        let (mut db, _dir) = create_test_db();
        let stored = db.store_embeddings(&[]).unwrap();
        assert_eq!(stored, 0);
        assert_eq!(db.embedding_count().unwrap(), 0);
    }

    #[test]
    fn test_store_replaces_existing() {
        let (mut db, _dir) = create_test_db();
        insert_test_symbol(&mut db, "sym1", "process_data", "src/lib.rs");

        // Store initial embedding
        db.store_embeddings(&[("sym1".to_string(), vec![0.1_f32; 384])])
            .unwrap();
        assert_eq!(db.embedding_count().unwrap(), 1);

        // Store replacement embedding (same symbol_id)
        db.store_embeddings(&[("sym1".to_string(), vec![0.9_f32; 384])])
            .unwrap();

        // Should still be 1, not 2
        assert_eq!(db.embedding_count().unwrap(), 1);
    }

    #[test]
    fn test_knn_search_returns_correct_order() {
        let (mut db, _dir) = create_test_db();
        insert_test_symbol(&mut db, "sym_close", "close_match", "src/lib.rs");
        insert_test_symbol(&mut db, "sym_far", "far_match", "src/lib.rs");

        // Create a target vector and two candidates at different distances
        let target = vec![1.0_f32; 384];
        let close = vec![0.9_f32; 384]; // closer to target
        let far = vec![0.1_f32; 384]; // farther from target

        db.store_embeddings(&[
            ("sym_close".to_string(), close),
            ("sym_far".to_string(), far),
        ])
        .unwrap();

        let results = db.knn_search(&target, 2).unwrap();
        assert_eq!(results.len(), 2);

        // First result should be the closer vector
        assert_eq!(results[0].0, "sym_close");
        assert_eq!(results[1].0, "sym_far");

        // Distance of close should be less than distance of far
        assert!(
            results[0].1 < results[1].1,
            "Close distance ({}) should be less than far distance ({})",
            results[0].1,
            results[1].1
        );
    }

    #[test]
    fn test_knn_search_respects_limit() {
        let (mut db, _dir) = create_test_db();

        // Insert 5 symbols
        let mut embeddings = Vec::new();
        for i in 0..5 {
            let id = format!("sym{i}");
            insert_test_symbol(&mut db, &id, &format!("func_{i}"), "src/lib.rs");
            embeddings.push((id, vec![(i as f32) * 0.1; 384]));
        }
        db.store_embeddings(&embeddings).unwrap();

        let results = db.knn_search(&[0.5_f32; 384], 3).unwrap();
        assert_eq!(results.len(), 3, "Should return at most 3 results");
    }

    #[test]
    fn test_knn_search_empty_table() {
        let (db, _dir) = create_test_db();
        let results = db.knn_search(&[0.1_f32; 384], 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_delete_embeddings_for_file() {
        let (mut db, _dir) = create_test_db();
        insert_test_symbol(&mut db, "sym1", "func_a", "src/main.rs");
        insert_test_symbol(&mut db, "sym2", "func_b", "src/main.rs");
        insert_test_symbol(&mut db, "sym3", "func_c", "src/other.rs");

        db.store_embeddings(&[
            ("sym1".to_string(), vec![0.1_f32; 384]),
            ("sym2".to_string(), vec![0.2_f32; 384]),
            ("sym3".to_string(), vec![0.3_f32; 384]),
        ])
        .unwrap();

        assert_eq!(db.embedding_count().unwrap(), 3);

        // Delete embeddings for src/main.rs
        let deleted = db.delete_embeddings_for_file("src/main.rs").unwrap();
        assert_eq!(deleted, 2);
        assert_eq!(db.embedding_count().unwrap(), 1);

        // sym3 should still be searchable
        let results = db.knn_search(&[0.3_f32; 384], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "sym3");
    }

    #[test]
    fn test_delete_embeddings_for_nonexistent_file() {
        let (mut db, _dir) = create_test_db();
        let deleted = db.delete_embeddings_for_file("no/such/file.rs").unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_clear_all_embeddings() {
        let (mut db, _dir) = create_test_db();
        insert_test_symbol(&mut db, "sym1", "func_a", "src/lib.rs");
        insert_test_symbol(&mut db, "sym2", "func_b", "src/lib.rs");

        db.store_embeddings(&[
            ("sym1".to_string(), vec![0.1_f32; 384]),
            ("sym2".to_string(), vec![0.2_f32; 384]),
        ])
        .unwrap();

        assert_eq!(db.embedding_count().unwrap(), 2);

        db.clear_all_embeddings().unwrap();
        assert_eq!(db.embedding_count().unwrap(), 0);
    }

    #[test]
    fn test_get_embedding_returns_stored_vector() {
        let (mut db, _dir) = create_test_db();
        insert_test_symbol(&mut db, "sym1", "process_data", "src/lib.rs");

        let embedding = vec![0.1_f32, 0.2, 0.3, 0.4, 0.5];
        // Pad to 384 dimensions (sqlite-vec schema requirement)
        let mut full_embedding = vec![0.0_f32; 384];
        for (i, &v) in embedding.iter().enumerate() {
            full_embedding[i] = v;
        }

        db.store_embeddings(&[("sym1".to_string(), full_embedding.clone())])
            .unwrap();

        let retrieved = db.get_embedding("sym1").unwrap();
        assert!(
            retrieved.is_some(),
            "Should return Some for stored embedding"
        );

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.len(), 384, "Should return 384-dimensional vector");

        // Verify first 5 values match what we stored
        for i in 0..5 {
            assert!(
                (retrieved[i] - full_embedding[i]).abs() < 1e-6,
                "Element {i} mismatch: got {}, expected {}",
                retrieved[i],
                full_embedding[i]
            );
        }
    }

    #[test]
    fn test_get_embedding_returns_none_for_missing() {
        let (db, _dir) = create_test_db();
        let retrieved = db.get_embedding("nonexistent_symbol").unwrap();
        assert!(
            retrieved.is_none(),
            "Should return None for non-existent symbol"
        );
    }

    #[test]
    fn test_get_embedding_rejects_malformed_blob_length() {
        let (db, _dir) = create_test_db();

        // Recreate symbol_vectors as a plain table so we can simulate a corrupted blob.
        db.conn.execute("DROP TABLE symbol_vectors", []).unwrap();
        db.conn
            .execute(
                "CREATE TABLE symbol_vectors (
                    symbol_id TEXT PRIMARY KEY,
                    embedding BLOB NOT NULL
                )",
                [],
            )
            .unwrap();

        // Insert malformed bytes (len=3, not divisible by 4).
        db.conn
            .execute(
                "INSERT INTO symbol_vectors(symbol_id, embedding) VALUES (?, ?)",
                rusqlite::params!["sym_bad", vec![0x01_u8, 0x02, 0x03]],
            )
            .unwrap();

        let err = db.get_embedding("sym_bad").unwrap_err();
        assert!(
            err.to_string().contains("Malformed embedding blob length"),
            "Expected malformed blob error, got: {err}"
        );
    }

    #[test]
    fn test_get_embedded_symbol_ids() {
        let (mut db, _dir) = create_test_db();
        insert_test_symbol(&mut db, "sym1", "func_a", "src/lib.rs");
        insert_test_symbol(&mut db, "sym2", "func_b", "src/lib.rs");
        insert_test_symbol(&mut db, "sym3", "func_c", "src/lib.rs");

        // No embeddings yet
        let ids = db.get_embedded_symbol_ids().unwrap();
        assert!(ids.is_empty(), "Should be empty when no embeddings stored");

        // Store embeddings for sym1 and sym3 (not sym2)
        db.store_embeddings(&[
            ("sym1".to_string(), vec![0.1_f32; 384]),
            ("sym3".to_string(), vec![0.3_f32; 384]),
        ])
        .unwrap();

        let ids = db.get_embedded_symbol_ids().unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("sym1"));
        assert!(ids.contains("sym3"));
        assert!(!ids.contains("sym2"), "sym2 should not be in the set");
    }

    #[test]
    fn test_migration_010_is_idempotent() {
        let (db, _dir) = create_test_db();

        // The migration ran during SymbolDatabase::new(). Verify the table exists.
        let table_exists: bool = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='symbol_vectors'",
                [],
                |row| row.get::<_, i32>(0).map(|c| c > 0),
            )
            .unwrap();

        assert!(
            table_exists,
            "symbol_vectors table should exist after migration"
        );

        // Verify schema version was bumped
        let version = db.get_schema_version().unwrap();
        assert_eq!(version, 10, "Schema version should be 10");
    }

    #[test]
    fn test_database_survives_close_and_reopen() {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");

        // Create DB, store embedding, close
        {
            let mut db = SymbolDatabase::new(&db_path).unwrap();
            insert_test_symbol(&mut db, "sym1", "func_a", "src/lib.rs");
            db.store_embeddings(&[("sym1".to_string(), vec![0.5_f32; 384])])
                .unwrap();
            assert_eq!(db.embedding_count().unwrap(), 1);
        }
        // db dropped here (connection closed)

        // Reopen and verify
        {
            let db = SymbolDatabase::new(&db_path).unwrap();
            assert_eq!(db.embedding_count().unwrap(), 1);

            let results = db.knn_search(&[0.5_f32; 384], 1).unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].0, "sym1");
        }
    }

    #[test]
    fn test_delete_embeddings_for_non_code_languages() {
        let (mut db, _dir) = create_test_db();

        // Insert symbols with different languages
        insert_test_symbol_with_lang(&mut db, "r1", "my_func", "src/lib.rs", "rust");
        insert_test_symbol_with_lang(&mut db, "md1", "Features", "README.md", "markdown");
        insert_test_symbol_with_lang(&mut db, "cs1", "MyClass", "src/Foo.cs", "csharp");
        insert_test_symbol_with_lang(&mut db, "json1", "config", "package.json", "json");
        insert_test_symbol_with_lang(&mut db, "toml1", "deps", "Cargo.toml", "toml");

        // Store embeddings for all
        let embeddings: Vec<_> = ["r1", "md1", "cs1", "json1", "toml1"]
            .iter()
            .map(|id| (id.to_string(), vec![0.1_f32; 384]))
            .collect();
        db.store_embeddings(&embeddings).unwrap();
        assert_eq!(db.embedding_count().unwrap(), 5);

        // Purge non-code languages
        let purged = db
            .delete_embeddings_for_languages(&["markdown", "json", "toml"])
            .unwrap();
        assert_eq!(purged, 3, "Should delete markdown + json + toml embeddings");
        assert_eq!(
            db.embedding_count().unwrap(),
            2,
            "Only rust + csharp should remain"
        );
    }
}
