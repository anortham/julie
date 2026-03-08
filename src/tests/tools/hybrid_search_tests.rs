/// Hybrid Search Tests — RRF Merge Algorithm and KNN Conversion
///
/// Tests for:
/// - Reciprocal Rank Fusion merge function (keyword + semantic)
/// - KNN-to-SymbolSearchResult conversion (embedding distances → search results)
///
/// Formula: RRF(d) = Σ 1/(k + rank) where rank is 1-based position.
#[cfg(test)]
mod tests {
    use crate::search::SymbolSearchResult;
    use crate::search::hybrid::rrf_merge;

    /// Helper to build a minimal SymbolSearchResult for testing.
    fn make_result(id: &str, name: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: String::new(),
            doc_comment: String::new(),
            file_path: format!("src/{}.rs", name),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_rrf_merge_disjoint_lists() {
        let tantivy = vec![
            make_result("1", "alpha", 10.0),
            make_result("2", "beta", 8.0),
        ];
        let semantic = vec![
            make_result("3", "gamma", 0.9),
            make_result("4", "delta", 0.7),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(merged.len(), 4, "all 4 disjoint items should appear");
        let ids: Vec<&str> = merged.iter().map(|r| r.id.as_str()).collect();
        assert!(ids.contains(&"1"));
        assert!(ids.contains(&"2"));
        assert!(ids.contains(&"3"));
        assert!(ids.contains(&"4"));
    }

    #[test]
    fn test_rrf_merge_overlapping_lists() {
        // "a" appears in both lists — should get boosted to rank 1
        let tantivy = vec![
            make_result("a", "shared", 10.0),
            make_result("b", "tantivy_only", 8.0),
        ];
        let semantic = vec![
            make_result("a", "shared", 0.95),
            make_result("c", "semantic_only", 0.8),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(merged.len(), 3, "3 unique items");
        assert_eq!(
            merged[0].id, "a",
            "overlapping item should rank first due to double RRF contribution"
        );
    }

    #[test]
    fn test_rrf_merge_respects_limit() {
        let tantivy = vec![
            make_result("1", "a", 10.0),
            make_result("2", "b", 8.0),
            make_result("3", "c", 6.0),
        ];
        let semantic = vec![make_result("4", "d", 0.9), make_result("5", "e", 0.7)];

        let merged = rrf_merge(tantivy, semantic, 60, 3);

        assert_eq!(merged.len(), 3, "should be capped at limit=3");
    }

    #[test]
    fn test_rrf_merge_empty_semantic() {
        let tantivy = vec![
            make_result("1", "alpha", 10.0),
            make_result("2", "beta", 8.0),
        ];
        let semantic = vec![];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(
            merged.len(),
            2,
            "graceful degradation: tantivy results unchanged"
        );
        assert_eq!(merged[0].id, "1");
        assert_eq!(merged[1].id, "2");
    }

    #[test]
    fn test_rrf_merge_empty_tantivy() {
        let tantivy = vec![];
        let semantic = vec![
            make_result("3", "gamma", 0.9),
            make_result("4", "delta", 0.7),
        ];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(
            merged.len(),
            2,
            "semantic results returned when tantivy empty"
        );
        assert_eq!(merged[0].id, "3");
        assert_eq!(merged[1].id, "4");
    }

    #[test]
    fn test_rrf_score_is_stored_in_result() {
        // Item in both lists at rank 1 (1-based), k=60
        // RRF score = 1/(60+1) + 1/(60+1) = 2/61
        let tantivy = vec![make_result("x", "shared", 10.0)];
        let semantic = vec![make_result("x", "shared", 0.95)];

        let merged = rrf_merge(tantivy, semantic, 60, 10);

        assert_eq!(merged.len(), 1);
        let expected_score = 2.0_f32 / 61.0;
        let actual_score = merged[0].score;
        assert!(
            (actual_score - expected_score).abs() < 1e-6,
            "RRF score should be 2/61 ≈ {:.6}, got {:.6}",
            expected_score,
            actual_score,
        );
    }
}

/// Hybrid search orchestrator tests.
///
/// Verifies that `hybrid_search` correctly:
/// - Returns keyword-only results when no embedding provider is given
/// - Degrades gracefully when the embedding provider fails
/// - Merges keyword + semantic results via RRF when both succeed
#[cfg(test)]
mod orchestrator_tests {
    use anyhow::Result;
    use std::io;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::writer::MakeWriter;

    use crate::database::SymbolDatabase;
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};
    use crate::search::hybrid::hybrid_search;
    use crate::search::index::{SearchFilter, SearchIndex, SymbolDocument, SymbolSearchResults};
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
        let db = SymbolDatabase::new(&db_dir.path().join("test.db")).unwrap();

        // Insert a file record for the foreign key constraint
        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES ('src/lib.rs', 'rust', 'abc123', 100, 0, 0)",
                [],
            )
            .unwrap();

        // Insert symbol into DB
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, file_path, language,
                 start_line, start_col, end_line, end_col, start_byte, end_byte,
                 reference_score, signature, doc_comment)
                 VALUES ('sym1', 'process_data', 'function', 'src/lib.rs', 'rust',
                 10, 0, 20, 0, 0, 200, 0.0,
                 'fn process_data(input: &str) -> Result<()>',
                 'Processes input data.')",
                [],
            )
            .unwrap();

        // Add symbol to Tantivy index
        index
            .add_symbol(&SymbolDocument {
                id: "sym1".into(),
                name: "process_data".into(),
                signature: "fn process_data(input: &str) -> Result<()>".into(),
                doc_comment: "Processes input data.".into(),
                code_body: "fn process_data(input: &str) -> Result<()> { Ok(()) }".into(),
                file_path: "src/lib.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 10,
            })
            .unwrap();
        index.commit().unwrap();

        (index, db, idx_dir, db_dir)
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
        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES ('scripts/tool.py', 'python', 'def456', 120, 0, 0)",
                [],
            )
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, file_path, language,
                 start_line, start_col, end_line, end_col, start_byte, end_byte,
                 reference_score, signature, doc_comment)
                 VALUES ('sym2', 'python_helper', 'function', 'scripts/tool.py', 'python',
                 5, 0, 15, 0, 0, 120, 0.0,
                 'def python_helper(data):',
                 'Python helper function.')",
                [],
            )
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
        };

        let provider = StaticProvider;
        let results =
            hybrid_search("process_data", &filter, 10, &index, &db, Some(&provider), None).unwrap();

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
}

/// Weighted RRF merge tests (Phase 5, Task 2).
///
/// Verifies that per-source weighting correctly biases the merge:
/// - Equal weights = same output as uniform merge
/// - Higher weight = more influence on ranking
/// - Zero weight = effectively excluded
#[cfg(test)]
mod weighted_rrf_tests {
    use crate::search::SymbolSearchResult;
    use crate::search::hybrid::{rrf_merge, weighted_rrf_merge};
    use crate::search::weights::SearchWeightProfile;

    fn make_result(id: &str, name: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: String::new(),
            doc_comment: String::new(),
            file_path: "test.rs".to_string(),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_weighted_equal_weights_matches_uniform() {
        let tantivy = vec![
            make_result("a", "alpha", 10.0),
            make_result("b", "beta", 8.0),
        ];
        let semantic = vec![
            make_result("b", "beta", 0.9),
            make_result("c", "gamma", 0.8),
        ];

        let tantivy_clone = tantivy.clone();
        let semantic_clone = semantic.clone();

        let uniform = rrf_merge(tantivy, semantic, 60, 10);
        let weighted = weighted_rrf_merge(tantivy_clone, semantic_clone, 60, 10, 1.0, 1.0);

        assert_eq!(uniform.len(), weighted.len());
        for (u, w) in uniform.iter().zip(weighted.iter()) {
            assert_eq!(u.id, w.id, "same order expected");
            assert!(
                (u.score - w.score).abs() < 1e-6,
                "scores should match: {} vs {}",
                u.score,
                w.score
            );
        }
    }

    #[test]
    fn test_weighted_higher_weight_increases_contribution() {
        // Two disjoint lists, each with one item
        let tantivy = vec![make_result("a", "alpha", 10.0)];
        let semantic = vec![make_result("b", "beta", 0.9)];

        // Heavy keyword weight
        let results =
            weighted_rrf_merge(tantivy.clone(), semantic.clone(), 60, 10, 2.0, 1.0);

        // "a" should rank higher because keyword weight is 2x
        assert_eq!(results[0].id, "a", "keyword result should rank first with 2x weight");

        // Now flip: heavy semantic weight
        let results2 =
            weighted_rrf_merge(tantivy, semantic, 60, 10, 1.0, 2.0);
        assert_eq!(results2[0].id, "b", "semantic result should rank first with 2x weight");
    }

    #[test]
    fn test_weighted_zero_weight_excludes_source() {
        let tantivy = vec![
            make_result("a", "alpha", 10.0),
            make_result("b", "beta", 8.0),
        ];
        let semantic = vec![
            make_result("c", "gamma", 0.9),
        ];

        // Zero semantic weight — only keyword results should have nonzero scores
        let results = weighted_rrf_merge(tantivy, semantic, 60, 10, 1.0, 0.0);

        // "c" only appeared in semantic with weight 0, so its score should be 0
        let gamma = results.iter().find(|r| r.id == "c").unwrap();
        assert!(
            gamma.score < 1e-10,
            "zero-weighted source items should have ~0 score, got {}",
            gamma.score
        );
    }

    #[test]
    fn test_search_weight_presets_have_expected_values() {
        let code = SearchWeightProfile::fast_search();
        assert!(code.keyword_weight >= 1.0, "fast_search should weight keywords strongly");
        assert!(code.semantic_weight > 0.0, "fast_search should still use semantic");

        let recall = SearchWeightProfile::recall();
        assert!(recall.keyword_weight > 0.0, "recall should use keywords");
        assert!(recall.semantic_weight >= 0.8, "recall should weight semantic strongly");

        let balanced = SearchWeightProfile::get_context();
        assert!(balanced.keyword_weight > 0.0);
        assert!(balanced.semantic_weight > 0.0);
    }
}

/// Weight profile wiring tests (Phase 5, Task 6).
///
/// Verifies that `hybrid_search` uses `weighted_rrf_merge` when a
/// `SearchWeightProfile` is provided, and falls back to uniform `rrf_merge`
/// when `None` is passed.
#[cfg(test)]
mod weight_profile_wiring_tests {
    use anyhow::Result;

    use crate::database::SymbolDatabase;
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};
    use crate::search::hybrid::hybrid_search;
    use crate::search::index::{SearchFilter, SearchIndex, SymbolDocument};
    use crate::search::weights::SearchWeightProfile;
    use tempfile::TempDir;

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

    /// Helper: create a SearchIndex + SymbolDatabase with test symbols + embeddings.
    fn setup_index_and_db_with_embeddings() -> (SearchIndex, SymbolDatabase, TempDir, TempDir) {
        let idx_dir = tempfile::tempdir().unwrap();
        let db_dir = tempfile::tempdir().unwrap();

        let index = SearchIndex::create(idx_dir.path()).unwrap();
        let mut db = SymbolDatabase::new(&db_dir.path().join("test.db")).unwrap();

        // Insert a file record for the foreign key constraint
        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES ('src/lib.rs', 'rust', 'abc123', 100, 0, 0)",
                [],
            )
            .unwrap();

        // Insert symbol into DB
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, file_path, language,
                 start_line, start_col, end_line, end_col, start_byte, end_byte,
                 reference_score, signature, doc_comment)
                 VALUES ('sym1', 'process_data', 'function', 'src/lib.rs', 'rust',
                 10, 0, 20, 0, 0, 200, 0.0,
                 'fn process_data(input: &str) -> Result<()>',
                 'Processes input data.')",
                [],
            )
            .unwrap();

        // Add symbol to Tantivy index
        index
            .add_symbol(&SymbolDocument {
                id: "sym1".into(),
                name: "process_data".into(),
                signature: "fn process_data(input: &str) -> Result<()>".into(),
                doc_comment: "Processes input data.".into(),
                code_body: "fn process_data(input: &str) -> Result<()> { Ok(()) }".into(),
                file_path: "src/lib.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 10,
            })
            .unwrap();
        index.commit().unwrap();

        // Seed embeddings so KNN returns results
        db.store_embeddings(&[("sym1".to_string(), vec![0.95_f32; 384])])
            .unwrap();

        (index, db, idx_dir, db_dir)
    }

    #[test]
    fn test_hybrid_search_with_weight_profile_uses_weighted_merge() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db_with_embeddings();
        let provider = StaticProvider;
        let profile = SearchWeightProfile::fast_search();

        let results = hybrid_search(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            Some(&provider),
            Some(profile),
        )
        .unwrap();

        assert!(
            !results.results.is_empty(),
            "should return results with weight profile"
        );
        assert_eq!(results.results[0].name, "process_data");
    }

    #[test]
    fn test_hybrid_search_none_profile_uses_uniform_merge() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db_with_embeddings();
        let provider = StaticProvider;

        // None profile should still work (backward compat)
        let results = hybrid_search(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            Some(&provider),
            None,
        )
        .unwrap();

        assert!(
            !results.results.is_empty(),
            "should return results with None profile"
        );
    }

    #[test]
    fn test_hybrid_search_weight_profile_keyword_only_graceful() {
        // When no embedding provider but profile is given, should still work
        let idx_dir = tempfile::tempdir().unwrap();
        let db_dir = tempfile::tempdir().unwrap();

        let index = SearchIndex::create(idx_dir.path()).unwrap();
        let db = SymbolDatabase::new(&db_dir.path().join("test.db")).unwrap();

        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES ('src/lib.rs', 'rust', 'abc123', 100, 0, 0)",
                [],
            )
            .unwrap();

        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, file_path, language,
                 start_line, start_col, end_line, end_col, start_byte, end_byte,
                 reference_score, signature, doc_comment)
                 VALUES ('sym1', 'process_data', 'function', 'src/lib.rs', 'rust',
                 10, 0, 20, 0, 0, 200, 0.0,
                 'fn process_data(input: &str) -> Result<()>',
                 'Processes input data.')",
                [],
            )
            .unwrap();

        index
            .add_symbol(&SymbolDocument {
                id: "sym1".into(),
                name: "process_data".into(),
                signature: "fn process_data(input: &str) -> Result<()>".into(),
                doc_comment: "Processes input data.".into(),
                code_body: "fn process_data(input: &str) -> Result<()> { Ok(()) }".into(),
                file_path: "src/lib.rs".into(),
                kind: "function".into(),
                language: "rust".into(),
                start_line: 10,
            })
            .unwrap();
        index.commit().unwrap();

        let profile = SearchWeightProfile::get_context();
        let results = hybrid_search(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            None, // No embedding provider
            Some(profile),
        )
        .unwrap();

        assert!(
            !results.results.is_empty(),
            "keyword-only should still work with a weight profile"
        );
    }
}

/// Characterization tests for `is_nl_like_query` — verifies the NL detection
/// heuristic that gates hybrid search activation in `fast_search`.
#[cfg(test)]
mod nl_query_detection_tests {
    use crate::search::scoring::is_nl_like_query;

    #[test]
    fn test_is_nl_like_query_examples() {
        // NL queries that SHOULD trigger hybrid search
        assert!(is_nl_like_query("how does the server start up"));
        assert!(is_nl_like_query("find symbols similar to each other"));
        assert!(is_nl_like_query("what happens when a file is modified"));

        // Code queries that should NOT trigger hybrid search
        assert!(!is_nl_like_query("UserService"));
        assert!(!is_nl_like_query("extract_identifiers"));
        assert!(!is_nl_like_query("rrf_merge"));
    }
}

/// KNN-to-SymbolSearchResult conversion tests.
///
/// Verifies that `knn_to_search_results` correctly converts (symbol_id, distance)
/// pairs from sqlite-vec into SymbolSearchResult objects by looking up metadata
/// from the database.
#[cfg(test)]
mod conversion_tests {
    use crate::database::SymbolDatabase;
    use crate::search::hybrid::knn_to_search_results;
    use tempfile::TempDir;

    /// Helper: create a fresh SymbolDatabase in a temp directory.
    fn create_test_db() -> (SymbolDatabase, TempDir) {
        let dir = tempfile::tempdir().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("Failed to create database");
        (db, dir)
    }

    /// Helper: insert a symbol with optional signature/doc_comment for testing.
    fn insert_test_symbol(
        db: &mut SymbolDatabase,
        id: &str,
        name: &str,
        kind: &str,
        language: &str,
        file_path: &str,
        start_line: u32,
        signature: Option<&str>,
        doc_comment: Option<&str>,
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
                "INSERT INTO symbols (id, name, kind, file_path, language,
                 start_line, start_col, end_line, end_col, start_byte, end_byte,
                 reference_score, signature, doc_comment)
                 VALUES (?, ?, ?, ?, ?, ?, 0, ?, 0, 0, 100, 0.0, ?, ?)",
                rusqlite::params![
                    id,
                    name,
                    kind,
                    file_path,
                    language,
                    start_line,
                    start_line + 10,
                    signature,
                    doc_comment
                ],
            )
            .expect("Failed to insert test symbol");
    }

    #[test]
    fn test_knn_to_search_results_converts_correctly() {
        let (mut db, _dir) = create_test_db();

        insert_test_symbol(
            &mut db,
            "sym1",
            "process_data",
            "function",
            "rust",
            "src/lib.rs",
            10,
            Some("fn process_data(input: &str) -> Result<()>"),
            Some("Processes input data."),
        );
        insert_test_symbol(
            &mut db,
            "sym2",
            "UserService",
            "struct",
            "rust",
            "src/service.rs",
            25,
            None,
            None,
        );

        // distance=0.1 → score=0.9, distance=0.3 → score=0.7
        let knn_results = vec![("sym1".to_string(), 0.1_f64), ("sym2".to_string(), 0.3_f64)];

        let results = knn_to_search_results(&knn_results, &db).unwrap();

        assert_eq!(results.len(), 2, "should convert both symbols");

        // First result: process_data
        assert_eq!(results[0].id, "sym1");
        assert_eq!(results[0].name, "process_data");
        assert_eq!(results[0].kind, "function");
        assert_eq!(results[0].language, "rust");
        assert_eq!(results[0].file_path, "src/lib.rs");
        assert_eq!(results[0].start_line, 10);
        assert_eq!(
            results[0].signature,
            "fn process_data(input: &str) -> Result<()>"
        );
        assert_eq!(results[0].doc_comment, "Processes input data.");
        assert!(
            (results[0].score - 0.9).abs() < 1e-5,
            "score should be 1.0 - 0.1 = 0.9, got {}",
            results[0].score
        );

        // Second result: UserService (no signature/doc_comment → empty strings)
        assert_eq!(results[1].id, "sym2");
        assert_eq!(results[1].name, "UserService");
        assert_eq!(results[1].kind, "struct");
        assert_eq!(results[1].signature, "");
        assert_eq!(results[1].doc_comment, "");
        assert!(
            (results[1].score - 0.7).abs() < 1e-5,
            "score should be 1.0 - 0.3 = 0.7, got {}",
            results[1].score
        );
    }

    #[test]
    fn test_knn_to_search_results_skips_missing_symbols() {
        let (mut db, _dir) = create_test_db();

        insert_test_symbol(
            &mut db,
            "sym1",
            "real_function",
            "function",
            "python",
            "src/main.py",
            1,
            None,
            None,
        );

        // Include a nonexistent ID between two real ones
        let knn_results = vec![
            ("sym1".to_string(), 0.2_f64),
            ("nonexistent_id".to_string(), 0.4_f64),
        ];

        let results = knn_to_search_results(&knn_results, &db).unwrap();

        assert_eq!(results.len(), 1, "should skip the missing symbol");
        assert_eq!(results[0].id, "sym1");
        assert_eq!(results[0].name, "real_function");
    }

    #[test]
    fn test_knn_to_search_results_empty_input() {
        let (db, _dir) = create_test_db();

        let knn_results: Vec<(String, f64)> = vec![];
        let results = knn_to_search_results(&knn_results, &db).unwrap();

        assert!(
            results.is_empty(),
            "empty input should produce empty output"
        );
    }
}
