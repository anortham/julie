//! Integration tests for incremental embedding via the file watcher pipeline.

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serial_test::serial;

    use crate::database::SymbolDatabase;
    use crate::embeddings::pipeline::{embed_symbols_for_file, reembed_symbols_for_file};
    use crate::embeddings::{DeviceInfo, EmbeddingProvider, OrtEmbeddingProvider};
    use crate::watcher::handlers;

    /// Helper: create a test database with a file and symbols.
    fn setup_db_with_file(
        dir: &std::path::Path,
        file_path: &str,
        symbols: &[(&str, &str, &str)], // (id, name, kind)
    ) -> Arc<Mutex<SymbolDatabase>> {
        let db_path = dir.join("test.db");
        let mut db = SymbolDatabase::new(&db_path).expect("create db");

        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES (?, 'rust', 'abc', 100, 0, 0)",
                [file_path],
            )
            .unwrap();

        for (id, name, kind) in symbols {
            db.conn
                .execute(
                    "INSERT INTO symbols (id, name, kind, file_path, language,
                     start_line, start_col, end_line, end_col, start_byte, end_byte,
                     reference_score)
                     VALUES (?, ?, ?, ?, 'rust', 1, 0, 10, 0, 0, 100, 0.0)",
                    rusqlite::params![id, name, kind, file_path],
                )
                .unwrap();
        }

        Arc::new(Mutex::new(db))
    }

    fn create_test_provider() -> OrtEmbeddingProvider {
        let cache_dir =
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
                .join(".cache")
                .join("fastembed");

        OrtEmbeddingProvider::try_new(Some(cache_dir)).expect("provider should init")
    }

    struct ShortBatchProvider;

    impl EmbeddingProvider for ShortBatchProvider {
        fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1_f32; 384])
        }

        fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            // Intentionally wrong count: always one vector regardless of input size.
            Ok(vec![vec![0.1_f32; 384]])
        }

        fn dimensions(&self) -> usize {
            384
        }

        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".to_string(),
                device: "cpu".to_string(),
                model_name: "short-batch".to_string(),
                dimensions: 384,
            }
        }
    }

    #[test]
    #[serial(fastembed)]
    fn test_embed_symbols_for_file_creates_embeddings() {
        let dir = tempfile::tempdir().unwrap();
        let db = setup_db_with_file(
            dir.path(),
            "src/lib.rs",
            &[
                ("s1", "process_data", "function"),
                ("s2", "UserService", "class"),
                ("s3", "my_var", "variable"), // not embeddable
            ],
        );

        let provider = create_test_provider();
        let count = embed_symbols_for_file(&db, &provider, "src/lib.rs").unwrap();

        assert_eq!(count, 2, "Should embed 2 of 3 symbols (skip variable)");

        let db_guard = db.lock().unwrap();
        assert_eq!(db_guard.embedding_count().unwrap(), 2);
    }

    #[test]
    #[serial(fastembed)]
    fn test_embed_symbols_for_file_with_no_embeddable_symbols() {
        let dir = tempfile::tempdir().unwrap();
        let db = setup_db_with_file(
            dir.path(),
            "src/lib.rs",
            &[("s1", "x", "variable"), ("s2", "os", "import")],
        );

        let provider = create_test_provider();
        let count = embed_symbols_for_file(&db, &provider, "src/lib.rs").unwrap();

        assert_eq!(
            count, 0,
            "No embeddable symbols should produce 0 embeddings"
        );
    }

    #[test]
    #[serial(fastembed)]
    fn test_delete_embeddings_before_file_delete() {
        let dir = tempfile::tempdir().unwrap();
        let db = setup_db_with_file(
            dir.path(),
            "src/main.rs",
            &[("s1", "main_func", "function"), ("s2", "Helper", "struct")],
        );

        let provider = create_test_provider();
        embed_symbols_for_file(&db, &provider, "src/main.rs").unwrap();

        {
            let db_guard = db.lock().unwrap();
            assert_eq!(db_guard.embedding_count().unwrap(), 2);
        }

        // Delete embeddings for the file (simulating what watcher does before file delete)
        {
            let mut db_guard = db.lock().unwrap();
            let deleted = db_guard.delete_embeddings_for_file("src/main.rs").unwrap();
            assert_eq!(deleted, 2);
            assert_eq!(db_guard.embedding_count().unwrap(), 0);
        }
    }

    #[test]
    #[serial(fastembed)]
    fn test_file_change_re_embeds() {
        let dir = tempfile::tempdir().unwrap();
        let db = setup_db_with_file(dir.path(), "src/lib.rs", &[("s1", "old_func", "function")]);

        let provider = create_test_provider();
        embed_symbols_for_file(&db, &provider, "src/lib.rs").unwrap();

        {
            let db_guard = db.lock().unwrap();
            assert_eq!(db_guard.embedding_count().unwrap(), 1);
        }

        // Simulate file change: delete old embeddings, update symbol, re-embed
        {
            let mut db_guard = db.lock().unwrap();
            db_guard.delete_embeddings_for_file("src/lib.rs").unwrap();
            // Update the symbol name (simulating a code change)
            db_guard
                .conn
                .execute("UPDATE symbols SET name = 'new_func' WHERE id = 's1'", [])
                .unwrap();
        }

        let count = embed_symbols_for_file(&db, &provider, "src/lib.rs").unwrap();
        assert_eq!(count, 1, "Should re-embed the updated symbol");

        let db_guard = db.lock().unwrap();
        assert_eq!(db_guard.embedding_count().unwrap(), 1);
    }

    #[test]
    #[serial(fastembed)]
    fn test_reembed_symbols_for_file_replaces_stale_embeddings() {
        let dir = tempfile::tempdir().unwrap();
        let db = setup_db_with_file(dir.path(), "src/lib.rs", &[("s1", "old_func", "function")]);

        let provider = create_test_provider();
        embed_symbols_for_file(&db, &provider, "src/lib.rs").unwrap();

        {
            let mut db_guard = db.lock().unwrap();
            // Simulate symbol replacement after file modification.
            db_guard
                .conn
                .execute("DELETE FROM symbols WHERE id = 's1'", [])
                .unwrap();
            db_guard
                .conn
                .execute(
                    "INSERT INTO symbols (id, name, kind, file_path, language,
                     start_line, start_col, end_line, end_col, start_byte, end_byte,
                     reference_score)
                     VALUES ('s2', 'new_func', 'function', 'src/lib.rs', 'rust',
                     1, 0, 10, 0, 0, 100, 0.0)",
                    [],
                )
                .unwrap();
        }

        reembed_symbols_for_file(&db, &provider, "src/lib.rs").unwrap();

        let db_guard = db.lock().unwrap();
        assert_eq!(
            db_guard.embedding_count().unwrap(),
            1,
            "Re-embedding should replace file embeddings instead of accumulating stale rows"
        );
    }

    #[test]
    fn test_embed_symbols_for_file_errors_on_vector_count_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let db = setup_db_with_file(
            dir.path(),
            "src/lib.rs",
            &[
                ("s1", "func_one", "function"),
                ("s2", "func_two", "function"),
            ],
        );

        let provider = ShortBatchProvider;
        let err = embed_symbols_for_file(&db, &provider, "src/lib.rs").unwrap_err();

        assert!(
            err.to_string().contains("Embedding count mismatch"),
            "Expected vector count mismatch error, got: {err}"
        );
    }
}
