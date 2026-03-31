#![cfg(feature = "embeddings-sidecar")]

//! Integration tests for incremental embedding via the file watcher pipeline,
//! using the fake sidecar provider.
//!
//! These are sidecar equivalents of the ORT-only tests in `embedding_incremental.rs`.

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::database::SymbolDatabase;
    use crate::embeddings::pipeline::{
        embed_symbols_for_file, reembed_symbols_for_file, run_embedding_pipeline,
    };
    use crate::tests::integration::sidecar_test_helpers::create_test_sidecar_provider;

    /// Helper: create a test database with a file and symbols.
    fn setup_db_with_file(
        dir: &std::path::Path,
        file_path: &str,
        symbols: &[(&str, &str, &str)], // (id, name, kind)
    ) -> Arc<Mutex<SymbolDatabase>> {
        let db_path = dir.join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("create db");

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

    #[test]
    fn test_sidecar_embed_symbols_for_file_creates_embeddings() {
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

        let provider = create_test_sidecar_provider();
        let count = embed_symbols_for_file(&db, &provider, "src/lib.rs", None).unwrap();

        assert_eq!(count, 2, "Should embed 2 of 3 symbols (skip variable)");

        let db_guard = db.lock().unwrap();
        assert_eq!(db_guard.embedding_count().unwrap(), 2);
    }

    #[test]
    fn test_sidecar_file_change_re_embeds() {
        let dir = tempfile::tempdir().unwrap();
        let db = setup_db_with_file(dir.path(), "src/lib.rs", &[("s1", "old_func", "function")]);

        let provider = create_test_sidecar_provider();
        embed_symbols_for_file(&db, &provider, "src/lib.rs", None).unwrap();

        {
            let db_guard = db.lock().unwrap();
            assert_eq!(db_guard.embedding_count().unwrap(), 1);
        }

        // Simulate file change: delete old embeddings, update symbol, re-embed
        {
            let mut db_guard = db.lock().unwrap();
            db_guard.delete_embeddings_for_file("src/lib.rs").unwrap();
            db_guard
                .conn
                .execute("UPDATE symbols SET name = 'new_func' WHERE id = 's1'", [])
                .unwrap();
        }

        let provider2 = create_test_sidecar_provider();
        let count = embed_symbols_for_file(&db, &provider2, "src/lib.rs", None).unwrap();
        assert_eq!(count, 1, "Should re-embed the updated symbol");

        let db_guard = db.lock().unwrap();
        assert_eq!(db_guard.embedding_count().unwrap(), 1);
    }

    #[test]
    fn test_sidecar_reembed_symbols_for_file_replaces_stale_embeddings() {
        let dir = tempfile::tempdir().unwrap();
        let db = setup_db_with_file(dir.path(), "src/lib.rs", &[("s1", "old_func", "function")]);

        let provider = create_test_sidecar_provider();
        embed_symbols_for_file(&db, &provider, "src/lib.rs", None).unwrap();

        {
            let db_guard = db.lock().unwrap();
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

        let provider2 = create_test_sidecar_provider();
        reembed_symbols_for_file(&db, &provider2, "src/lib.rs", None).unwrap();

        let db_guard = db.lock().unwrap();
        assert_eq!(
            db_guard.embedding_count().unwrap(),
            1,
            "Re-embedding should replace file embeddings instead of accumulating stale rows"
        );
    }

    #[test]
    fn test_sidecar_pipeline_with_file_level_embeddings() {
        let dir = tempfile::tempdir().unwrap();
        let db = setup_db_with_file(
            dir.path(),
            "src/lib.rs",
            &[
                ("s1", "process_data", "function"),
                ("s2", "UserService", "struct"),
                ("s3", "my_var", "variable"), // not embeddable
            ],
        );

        let provider = create_test_sidecar_provider();
        let stats = run_embedding_pipeline(&db, &provider, None).unwrap();

        assert_eq!(
            stats.symbols_embedded, 2,
            "Pipeline should embed 2 embeddable symbols"
        );

        let db_guard = db.lock().unwrap();
        assert_eq!(db_guard.embedding_count().unwrap(), 2);
        assert!(
            db_guard.get_embedding("s1").unwrap().is_some(),
            "function should have a vector"
        );
        assert!(
            db_guard.get_embedding("s2").unwrap().is_some(),
            "struct should have a vector"
        );
        assert!(
            db_guard.get_embedding("s3").unwrap().is_none(),
            "variable should not have a vector"
        );
    }
}
