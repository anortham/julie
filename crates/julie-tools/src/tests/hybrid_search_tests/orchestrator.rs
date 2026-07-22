/// Hybrid search orchestrator tests.
///
/// Verifies that `hybrid_search` correctly:
/// - Returns keyword-only results when no embedding provider is given
/// - Degrades gracefully when the embedding provider fails
/// - Merges keyword + semantic results via RRF when both succeed
#[cfg(test)]
mod orchestrator_tests {
    use anyhow::Result;
    use std::collections::HashMap;
    use std::io;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::writer::MakeWriter;

    use julie_core::database::SymbolDatabase;
    use julie_extractors::SymbolKind;
    use julie_index::search::hybrid::hybrid_search;
    use julie_index::search::index::{
        SearchDocument, SearchFilter, SearchIndex, SymbolSearchResults,
    };
    use julie_pipeline::embeddings::{DeviceInfo, EmbeddingProvider};
    use julie_test_support::db::{file_info_builder, store_file_info_if_missing, symbol_builder};
    use tempfile::TempDir;

    #[derive(Clone)]
    struct LogCaptureWriter {
        buf: Arc<Mutex<Vec<u8>>>,
    }

    struct LogCaptureGuard {
        buf: Arc<Mutex<Vec<u8>>>,
    }

    impl<'a> MakeWriter<'a> for LogCaptureWriter {
        type Writer = LogCaptureGuard;

        fn make_writer(&'a self) -> Self::Writer {
            LogCaptureGuard {
                buf: Arc::clone(&self.buf),
            }
        }
    }

    impl Write for LogCaptureGuard {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let mut guard = self
                .buf
                .lock()
                .map_err(|_| io::Error::other("log buffer mutex poisoned"))?;
            guard.extend_from_slice(data);
            Ok(data.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn run_hybrid_search_with_warn_capture<F>(run: F) -> (Result<SymbolSearchResults>, String)
    where
        F: FnOnce() -> Result<SymbolSearchResults>,
    {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = LogCaptureWriter {
            buf: Arc::clone(&buffer),
        };

        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_ansi(false)
            .with_writer(writer)
            .finish();

        let results = tracing::subscriber::with_default(subscriber, run);

        let logs = String::from_utf8(
            buffer
                .lock()
                .expect("log capture mutex should not be poisoned")
                .clone(),
        )
        .expect("captured logs should be utf-8");

        (results, logs)
    }

    /// Mock embedding provider that always fails (for degradation testing).
    struct FailingProvider;

    impl EmbeddingProvider for FailingProvider {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
            anyhow::bail!("embedding model not loaded")
        }
        fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            anyhow::bail!("embedding model not loaded")
        }
        fn dimensions(&self) -> usize {
            384
        }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".into(),
                device: "cpu".into(),
                model_name: "failing-mock".into(),
                dimensions: 384,
            }
        }
    }

    /// Mock sidecar provider that simulates query timeout.
    struct SidecarTimeoutProvider;

    impl EmbeddingProvider for SidecarTimeoutProvider {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
            anyhow::bail!(
                "timed out waiting for sidecar response for method 'embed_query' after 50ms"
            )
        }

        fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            anyhow::bail!(
                "timed out waiting for sidecar response for method 'embed_batch' after 50ms"
            )
        }

        fn dimensions(&self) -> usize {
            384
        }

        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "python-sidecar".into(),
                device: "cpu".into(),
                model_name: "fake-sidecar-timeout".into(),
                dimensions: 384,
            }
        }
    }

    /// Mock embedding provider that returns a deterministic vector.
    struct StaticProvider;

    impl EmbeddingProvider for StaticProvider {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![1.0_f32; 384])
        }

        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![1.0_f32; 384]).collect())
        }

        fn dimensions(&self) -> usize {
            384
        }

        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".into(),
                device: "cpu".into(),
                model_name: "static-mock".into(),
                dimensions: 384,
            }
        }
    }

    /// Helper: create a SearchIndex + SymbolDatabase with a few symbols.
    fn setup_index_and_db() -> (SearchIndex, SymbolDatabase, TempDir, TempDir) {
        let idx_dir = tempfile::tempdir().unwrap();
        let db_dir = tempfile::tempdir().unwrap();

        let index = SearchIndex::create(idx_dir.path()).unwrap();
        let mut db = SymbolDatabase::new(&db_dir.path().join("test.db")).unwrap();

        store_file_info(&db, "src/lib.rs", "rust", "abc123", 100);
        db.store_symbols(&[symbol_builder("sym1", "process_data", "src/lib.rs")
            .kind(SymbolKind::Function)
            .language("rust")
            .span(10, 0, 20, 0)
            .bytes(0, 200)
            .signature("fn process_data(input: &str) -> Result<()>")
            .doc_comment("Processes input data.")
            .confidence(1.0)
            .build()])
            .unwrap();

        // Add symbol to Tantivy index
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "sym1",
                "process_data",
                "fn process_data(input: &str) -> Result<()>",
                "Processes input data.",
                "fn process_data(input: &str) -> Result<()> { Ok(()) }",
                "src/lib.rs",
                "function",
                "rust",
                10,
            ))
            .unwrap();
        index.commit().unwrap();

        (index, db, idx_dir, db_dir)
    }

    fn store_file_info(
        db: &SymbolDatabase,
        file_path: &str,
        language: &str,
        hash: &str,
        size: i64,
    ) {
        store_file_info_if_missing(
            db,
            &file_info_builder(file_path)
                .language(language)
                .hash(hash)
                .size(size)
                .last_modified(0)
                .last_indexed(0)
                .symbol_count(0)
                .line_count(0)
                .build(),
        )
        .unwrap();
    }

    #[test]
    fn test_hybrid_search_none_provider_returns_keyword_results() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db();

        let results = hybrid_search(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            None, // No embedding provider
            None, // No weight profile
        )
        .unwrap();

        assert!(!results.results.is_empty(), "should find keyword results");
        assert_eq!(results.results[0].name, "process_data");
    }

    #[test]
    fn test_hybrid_search_failing_provider_degrades_gracefully() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db();
        let failing = FailingProvider;

        let results = hybrid_search(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            Some(&failing),
            None, // No weight profile
        )
        .unwrap(); // Must NOT fail despite provider error

        assert!(
            !results.results.is_empty(),
            "should still return keyword results"
        );
        assert_eq!(results.results[0].name, "process_data");
    }

    #[test]
    fn test_hybrid_search_preserves_relaxed_flag() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db();

        let results = hybrid_search(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            None,
            None,
        )
        .unwrap();

        // relaxed should be false for a simple single-term query that matches
        assert!(
            !results.relaxed,
            "relaxed flag should be preserved from tantivy"
        );
    }

    #[test]
    fn test_hybrid_search_filters_semantic_results_by_language_and_file_pattern() {
        let (index, mut db, _idx_dir, _db_dir) = setup_index_and_db();

        // Add a symbol that should be excluded by both language + file_pattern filters.
        store_file_info(&mut db, "scripts/tool.py", "python", "def456", 120);
        db.store_symbols(&[symbol_builder("sym2", "python_helper", "scripts/tool.py")
            .kind(SymbolKind::Function)
            .language("python")
            .span(5, 0, 15, 0)
            .bytes(0, 120)
            .signature("def python_helper(data):")
            .doc_comment("Python helper function.")
            .confidence(1.0)
            .build()])
            .unwrap();

        // Seed embeddings so KNN returns both symbols.
        db.store_embeddings(&[
            ("sym1".to_string(), vec![0.95_f32; 384]),
            ("sym2".to_string(), vec![1.0_f32; 384]),
        ])
        .unwrap();

        let filter = SearchFilter {
            language: Some("rust".to_string()),
            kind: None,
            file_pattern: Some("src/**/*.rs".to_string()),
            exclude_tests: false,
        };

        let provider = StaticProvider;
        let results = hybrid_search(
            "process_data",
            &filter,
            10,
            &index,
            &db,
            Some(&provider),
            None,
        )
        .unwrap();

        assert!(!results.results.is_empty(), "expected at least one result");
        assert!(
            results.results.iter().all(|r| r.language == "rust"),
            "all results should respect language filter"
        );
        assert!(
            results
                .results
                .iter()
                .all(|r| r.file_path.starts_with("src/") && r.file_path.ends_with(".rs")),
            "all results should respect file_pattern filter"
        );
    }

    #[test]
    fn test_hybrid_search_sidecar_timeout_degrades_to_keyword_results() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db();
        let timeout = SidecarTimeoutProvider;

        let (results, logs) = run_hybrid_search_with_warn_capture(|| {
            hybrid_search(
                "process_data",
                &SearchFilter::default(),
                10,
                &index,
                &db,
                Some(&timeout),
                None,
            )
        });
        let results = results.unwrap();

        assert!(
            !results.results.is_empty(),
            "should still return keyword results"
        );
        assert_eq!(results.results[0].name, "process_data");
        assert!(
            logs.contains("Semantic search failed, falling back to keyword-only"),
            "expected fallback warning log, got: {logs}"
        );
    }

    #[test]
    fn test_hybrid_search_exclude_tests_filters_semantic_results() {
        let (index, mut db, _idx_dir, _db_dir) = setup_index_and_db();

        // Insert a test-file symbol into DB
        store_file_info(
            &mut db,
            "src/tests/pipeline_tests.rs",
            "rust",
            "testfile123",
            80,
        );
        db.store_symbols(&[symbol_builder(
            "test_fn",
            "test_process_data",
            "src/tests/pipeline_tests.rs",
        )
        .kind(SymbolKind::Function)
        .language("rust")
        .span(5, 0, 15, 0)
        .bytes(0, 100)
        .signature("fn test_process_data()")
        .doc_comment("")
        .confidence(1.0)
        .build()])
            .unwrap();

        // Do NOT add the test symbol to Tantivy — it should only be reachable
        // via the semantic (KNN) path, so the only way it enters the merge is
        // through matches_filter on semantic candidates.

        // Store 384-dim embeddings for both symbols so the semantic path finds them.
        // The test symbol gets a slightly higher embedding similarity score.
        let prod_vec: Vec<f32> = (0..384).map(|i| if i == 0 { 0.8 } else { 0.0 }).collect();
        let test_vec: Vec<f32> = (0..384).map(|i| if i == 0 { 0.9 } else { 0.0 }).collect();
        db.store_embeddings(&[
            ("test_fn".to_string(), test_vec),
            ("sym1".to_string(), prod_vec),
        ])
        .unwrap();

        // Provider that returns a query vector close to both stored embeddings
        struct TestProvider;
        impl EmbeddingProvider for TestProvider {
            fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
                Ok((0..384).map(|i| if i == 0 { 0.85 } else { 0.0 }).collect())
            }
            fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
                Ok(texts
                    .iter()
                    .map(|_| (0..384).map(|i| if i == 0 { 0.85 } else { 0.0 }).collect())
                    .collect())
            }
            fn dimensions(&self) -> usize {
                384
            }
            fn device_info(&self) -> DeviceInfo {
                DeviceInfo {
                    runtime: "test".into(),
                    device: "cpu".into(),
                    model_name: "test-provider".into(),
                    dimensions: 384,
                }
            }
        }

        let filter = SearchFilter {
            exclude_tests: true,
            ..Default::default()
        };

        let results = hybrid_search(
            "process data",
            &filter,
            10,
            &index,
            &db,
            Some(&TestProvider),
            None,
        )
        .unwrap();

        // The test-file symbol should have been filtered out by matches_filter
        // BEFORE the RRF merge, not pushed out after damage is done
        for r in &results.results {
            assert!(
                !r.file_path.contains("tests/"),
                "Test file result '{}' in '{}' should have been filtered from semantic candidates",
                r.name,
                r.file_path
            );
        }
    }

    #[test]
    fn test_hybrid_search_exclude_tests_filters_metadata_test_semantic_results() {
        let (index, mut db, _idx_dir, _db_dir) = setup_index_and_db();

        let metadata: HashMap<String, serde_json::Value> =
            serde_json::from_str(r#"{"is_test":true,"test_role":"impl_test"}"#).unwrap();
        db.store_symbols(&[symbol_builder(
            "inline_test_fn",
            "inline_process_data_test",
            "src/lib.rs",
        )
        .kind(SymbolKind::Function)
        .language("rust")
        .span(30, 0, 40, 0)
        .bytes(0, 100)
        .signature("fn inline_process_data_test()")
        .doc_comment("")
        .metadata(metadata)
        .confidence(1.0)
        .build()])
            .unwrap();

        let prod_vec: Vec<f32> = (0..384).map(|i| if i == 0 { 0.8 } else { 0.0 }).collect();
        let test_vec: Vec<f32> = (0..384).map(|i| if i == 0 { 0.9 } else { 0.0 }).collect();
        db.store_embeddings(&[
            ("inline_test_fn".to_string(), test_vec),
            ("sym1".to_string(), prod_vec),
        ])
        .unwrap();

        struct TestProvider;
        impl EmbeddingProvider for TestProvider {
            fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
                Ok((0..384).map(|i| if i == 0 { 0.85 } else { 0.0 }).collect())
            }

            fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
                Ok(texts
                    .iter()
                    .map(|_| (0..384).map(|i| if i == 0 { 0.85 } else { 0.0 }).collect())
                    .collect())
            }

            fn dimensions(&self) -> usize {
                384
            }

            fn device_info(&self) -> DeviceInfo {
                DeviceInfo {
                    runtime: "test".into(),
                    device: "cpu".into(),
                    model_name: "test-provider".into(),
                    dimensions: 384,
                }
            }
        }

        let filter = SearchFilter {
            exclude_tests: true,
            ..Default::default()
        };

        let results = hybrid_search(
            "process data",
            &filter,
            10,
            &index,
            &db,
            Some(&TestProvider),
            None,
        )
        .unwrap();

        assert!(
            results
                .results
                .iter()
                .all(|result| result.id != "inline_test_fn"),
            "metadata-only inline test symbol leaked through semantic results: {:?}",
            results
                .results
                .iter()
                .map(|result| (&result.id, &result.name, &result.file_path, &result.role))
                .collect::<Vec<_>>()
        );
    }
}
