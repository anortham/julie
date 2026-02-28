#![cfg(feature = "embeddings-ort")]

//! Integration tests for the background embedding pipeline.

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serial_test::serial;

    use crate::database::SymbolDatabase;
    use crate::embeddings::pipeline::run_embedding_pipeline;
    use crate::embeddings::{EmbeddingProvider, OrtEmbeddingProvider};
    use crate::extractors::SymbolKind;

    /// Helper: create a test database with symbols.
    fn setup_db_with_symbols(symbols: &[(&str, &str, SymbolKind)]) -> Arc<Mutex<SymbolDatabase>> {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).expect("create db");

        // Insert file record
        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES ('src/lib.rs', 'rust', 'abc', 100, 0, 0)",
                [],
            )
            .unwrap();

        // Insert symbols (all NOT NULL columns + the columns row_to_symbol reads)
        for (id, name, kind) in symbols {
            db.conn
                .execute(
                    "INSERT INTO symbols (id, name, kind, file_path, language,
                     start_line, start_col, end_line, end_col, start_byte, end_byte,
                     reference_score)
                     VALUES (?, ?, ?, 'src/lib.rs', 'rust', 1, 0, 10, 0, 0, 100, 0.0)",
                    rusqlite::params![id, name, format!("{:?}", kind).to_lowercase()],
                )
                .unwrap();
        }

        // Leak the tempdir so it persists (db holds the file open)
        std::mem::forget(dir);

        Arc::new(Mutex::new(db))
    }

    /// Helper: create the test embedding provider.
    fn create_test_provider() -> OrtEmbeddingProvider {
        let cache_dir =
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
                .join(".cache")
                .join("fastembed");

        OrtEmbeddingProvider::try_new(Some(cache_dir)).expect("provider should init")
    }

    #[test]
    #[serial(fastembed)]
    fn test_pipeline_embeds_correct_count() {
        let db = setup_db_with_symbols(&[
            ("s1", "process_data", SymbolKind::Function),
            ("s2", "UserService", SymbolKind::Class),
            ("s3", "my_var", SymbolKind::Variable), // not embeddable
            ("s4", "handle_error", SymbolKind::Method),
            ("s5", "os", SymbolKind::Import), // not embeddable
        ]);

        let provider = create_test_provider();
        let stats = run_embedding_pipeline(&db, &provider).unwrap();

        assert_eq!(stats.symbols_scanned, 5, "Should scan all 5 symbols");
        assert_eq!(
            stats.symbols_embedded, 3,
            "Should embed 3 embeddable symbols"
        );
        assert!(stats.batches_processed >= 1);

        // Verify embeddings are stored
        let db_guard = db.lock().unwrap();
        assert_eq!(db_guard.embedding_count().unwrap(), 3);
    }

    #[test]
    #[serial(fastembed)]
    fn test_pipeline_knn_works_after_embedding() {
        let db = setup_db_with_symbols(&[
            ("s1", "authenticate_user", SymbolKind::Function),
            ("s2", "DatabaseConnection", SymbolKind::Struct),
            ("s3", "parse_json_data", SymbolKind::Function),
        ]);

        let provider = create_test_provider();
        run_embedding_pipeline(&db, &provider).unwrap();

        // Search for something semantically related to authentication
        let query_vec = provider.embed_query("login and user verification").unwrap();

        let db_guard = db.lock().unwrap();
        let results = db_guard.knn_search(&query_vec, 3).unwrap();

        assert_eq!(results.len(), 3, "Should return all 3 results");
        // authenticate_user should be closest to "login and user verification"
        assert_eq!(
            results[0].0, "s1",
            "authenticate_user should be the closest match for 'login and user verification'"
        );
    }

    #[test]
    #[serial(fastembed)]
    fn test_pipeline_empty_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let db = Arc::new(Mutex::new(db));

        let provider = create_test_provider();
        let stats = run_embedding_pipeline(&db, &provider).unwrap();

        assert_eq!(stats.symbols_scanned, 0);
        assert_eq!(stats.symbols_embedded, 0);
    }

    #[test]
    #[serial(fastembed)]
    fn test_pipeline_skips_already_embedded() {
        let db = setup_db_with_symbols(&[
            ("s1", "process_data", SymbolKind::Function),
            ("s2", "UserService", SymbolKind::Class),
            ("s3", "handle_error", SymbolKind::Method),
        ]);

        // Pre-store dummy embeddings for s1 and s2
        {
            let mut db_guard = db.lock().unwrap();
            db_guard
                .store_embeddings(&[
                    ("s1".to_string(), vec![0.1_f32; 384]),
                    ("s2".to_string(), vec![0.2_f32; 384]),
                ])
                .unwrap();
            assert_eq!(db_guard.embedding_count().unwrap(), 2);
        }

        // Run pipeline — should only embed s3 (the one without a vector)
        let provider = create_test_provider();
        let stats = run_embedding_pipeline(&db, &provider).unwrap();

        assert_eq!(stats.symbols_skipped, 2, "Should skip s1 and s2");
        assert_eq!(stats.symbols_embedded, 1, "Should embed only s3");

        let db_guard = db.lock().unwrap();
        assert_eq!(
            db_guard.embedding_count().unwrap(),
            3,
            "Total should be 3 (2 pre-existing + 1 new)"
        );
    }

    #[test]
    #[serial(fastembed)]
    fn test_pipeline_embedding_count_matches() {
        let db = setup_db_with_symbols(&[
            ("s1", "MyTrait", SymbolKind::Trait),
            ("s2", "MyEnum", SymbolKind::Enum),
            ("s3", "my_const", SymbolKind::Constant), // not embeddable
        ]);

        let provider = create_test_provider();
        let stats = run_embedding_pipeline(&db, &provider).unwrap();

        let db_guard = db.lock().unwrap();
        let count = db_guard.embedding_count().unwrap();

        assert_eq!(
            count as usize, stats.symbols_embedded,
            "embedding_count should match stats"
        );
        assert_eq!(count, 2, "Should have 2 embeddings (Trait + Enum)");
    }
}
