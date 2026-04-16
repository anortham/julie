#![cfg(feature = "embeddings-sidecar")]

//! Integration tests for the background embedding pipeline using the fake sidecar provider.
//!
//! These are sidecar equivalents of the ORT-only tests in `embedding_pipeline.rs`.

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::database::SymbolDatabase;
    use crate::embeddings::EmbeddingProvider;
    use crate::embeddings::pipeline::run_embedding_pipeline;
    use crate::extractors::SymbolKind;
    use crate::tests::integration::sidecar_test_helpers::{
        create_test_sidecar_provider, create_test_sidecar_provider_with_health_result,
    };

    /// Helper: create a test database with symbols.
    fn setup_db_with_symbols(symbols: &[(&str, &str, SymbolKind)]) -> Arc<Mutex<SymbolDatabase>> {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("create db");

        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES ('src/lib.rs', 'rust', 'abc', 100, 0, 0)",
                [],
            )
            .unwrap();

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

    #[test]
    fn test_sidecar_pipeline_embeds_correct_count() {
        let db = setup_db_with_symbols(&[
            ("s1", "process_data", SymbolKind::Function),
            ("s2", "UserService", SymbolKind::Class),
            ("s3", "my_var", SymbolKind::Variable), // not embeddable
            ("s4", "handle_error", SymbolKind::Method),
            ("s5", "os", SymbolKind::Import), // not embeddable
        ]);

        let provider = create_test_sidecar_provider();
        let stats = run_embedding_pipeline(&db, &provider, None).unwrap();

        assert_eq!(stats.symbols_scanned, 5, "Should scan all 5 symbols");
        assert_eq!(
            stats.symbols_embedded, 3,
            "Should embed 3 embeddable symbols"
        );
        assert!(stats.batches_processed >= 1);

        let db_guard = db.lock().unwrap();
        assert_eq!(db_guard.embedding_count().unwrap(), 3);
    }

    #[test]
    fn test_sidecar_pipeline_empty_database() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();
        let db = Arc::new(Mutex::new(db));

        let provider = create_test_sidecar_provider();
        let stats = run_embedding_pipeline(&db, &provider, None).unwrap();

        assert_eq!(stats.symbols_scanned, 0);
        assert_eq!(stats.symbols_embedded, 0);
    }

    #[test]
    fn test_sidecar_pipeline_skips_already_embedded() {
        let db = setup_db_with_symbols(&[
            ("s1", "process_data", SymbolKind::Function),
            ("s2", "UserService", SymbolKind::Class),
            ("s3", "handle_error", SymbolKind::Method),
        ]);

        // Pre-store dummy embeddings for s1 and s2, and set config to match the
        // sidecar provider so the pipeline doesn't treat this as a model change.
        {
            let mut db_guard = db.lock().unwrap();
            // The fake sidecar returns model_id=None in its health response, so the
            // provider falls back to "BAAI/bge-small-en-v1.5".
            db_guard
                .set_embedding_config(
                    "BAAI/bge-small-en-v1.5",
                    384,
                    crate::embeddings::pipeline::EMBEDDING_FORMAT_VERSION,
                )
                .unwrap();
            db_guard
                .store_embeddings(&[
                    ("s1".to_string(), vec![0.1_f32; 384]),
                    ("s2".to_string(), vec![0.2_f32; 384]),
                ])
                .unwrap();
            assert_eq!(db_guard.embedding_count().unwrap(), 2);
        }

        // Run pipeline — should embed s3 (new) + re-embed s2 (container symbol,
        // always re-embedded because child method enrichment may change).
        // s1 (Function) is skipped because it's already embedded and not a container.
        let provider = create_test_sidecar_provider();
        let stats = run_embedding_pipeline(&db, &provider, None).unwrap();

        assert_eq!(
            stats.symbols_embedded, 2,
            "Should embed s3 (new) + s2 (container re-embed)"
        );

        let db_guard = db.lock().unwrap();
        assert_eq!(
            db_guard.embedding_count().unwrap(),
            3,
            "Total should be 3 (s1 pre-existing + s2 re-embedded + s3 new)"
        );
    }

    #[test]
    fn test_sidecar_pipeline_reembeds_on_model_name_change() {
        let db = setup_db_with_symbols(&[
            ("s1", "process_data", SymbolKind::Function),
            ("s2", "UserService", SymbolKind::Class),
            ("s3", "handle_error", SymbolKind::Method),
        ]);

        // First run: embed everything with the sidecar provider
        let provider = create_test_sidecar_provider();
        let stats = run_embedding_pipeline(&db, &provider, None).unwrap();
        assert_eq!(stats.symbols_embedded, 3);

        {
            let db_guard = db.lock().unwrap();
            assert_eq!(db_guard.embedding_count().unwrap(), 3);
        }

        // Simulate a model switch: change stored model name to something different,
        // keeping the same 384 dimensions (the bug case).
        {
            let mut db_guard = db.lock().unwrap();
            db_guard
                .set_embedding_config(
                    "fake-old-model/v1",
                    384,
                    crate::embeddings::pipeline::EMBEDDING_FORMAT_VERSION,
                )
                .unwrap();
        }

        // Second run: pipeline should detect model name mismatch, wipe all vectors,
        // and re-embed everything from scratch.
        let provider2 = create_test_sidecar_provider();
        let stats = run_embedding_pipeline(&db, &provider2, None).unwrap();
        assert_eq!(
            stats.symbols_embedded, 3,
            "All symbols should be re-embedded after model change, not skipped"
        );

        let db_guard = db.lock().unwrap();
        assert_eq!(db_guard.embedding_count().unwrap(), 3);
        let (model, dims, _fmt_ver) = db_guard.get_embedding_config().unwrap();
        assert_eq!(dims, 384);
        assert_ne!(model, "fake-old-model/v1", "Model name should be updated");
    }

    #[test]
    fn test_sidecar_provider_health_probe_accepts_structured_capability_payload() {
        let provider = create_test_sidecar_provider_with_health_result(
            r#"{
                "ready": true,
                "runtime": "fake-sidecar",
                "device": "cpu",
                "dims": 384,
                "resolved_backend": "sidecar",
                "accelerated": false,
                "degraded_reason": null,
                "capabilities": {
                    "cpu": {"available": true},
                    "cuda": {"available": false},
                    "directml": {"available": true},
                    "mps": {"available": false}
                },
                "load_policy": {
                    "requested_device_backend": "cpu",
                    "resolved_device_backend": "cpu"
                }
            }"#,
        );

        assert_eq!(provider.dimensions(), 384);
        assert_eq!(provider.device_info().device, "cpu");
        assert_eq!(provider.accelerated(), Some(false));
    }

    #[test]
    fn test_sidecar_provider_health_probe_preserves_degraded_runtime_reason() {
        let provider = create_test_sidecar_provider_with_health_result(
            r#"{
                "ready": true,
                "runtime": "fake-sidecar",
                "device": "cpu",
                "dims": 384,
                "resolved_backend": "sidecar",
                "accelerated": false,
                "degraded_reason": "probe encode failed on directml, fell back to CPU",
                "capabilities": {
                    "cpu": {"available": true},
                    "cuda": {"available": false},
                    "directml": {"available": true},
                    "mps": {"available": false}
                },
                "load_policy": {
                    "requested_device_backend": "directml",
                    "resolved_device_backend": "cpu"
                }
            }"#,
        );

        assert_eq!(
            provider.degraded_reason().as_deref(),
            Some("probe encode failed on directml, fell back to CPU")
        );
        assert_eq!(
            provider.device_info().runtime,
            "python-sidecar (fake-sidecar)"
        );
    }
}
