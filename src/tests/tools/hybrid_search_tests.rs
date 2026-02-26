/// Hybrid Search Tests — RRF Merge Algorithm and KNN Conversion
///
/// Tests for:
/// - Reciprocal Rank Fusion merge function (keyword + semantic)
/// - KNN-to-SymbolSearchResult conversion (embedding distances → search results)
///
/// Formula: RRF(d) = Σ 1/(k + rank) where rank is 1-based position.
#[cfg(test)]
mod tests {
    use crate::search::hybrid::rrf_merge;
    use crate::search::SymbolSearchResult;

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
        let semantic = vec![
            make_result("4", "d", 0.9),
            make_result("5", "e", 0.7),
        ];

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

        assert_eq!(merged.len(), 2, "graceful degradation: tantivy results unchanged");
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

        assert_eq!(merged.len(), 2, "semantic results returned when tantivy empty");
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

    use crate::database::SymbolDatabase;
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};
    use crate::search::hybrid::hybrid_search;
    use crate::search::index::{SearchFilter, SearchIndex, SymbolDocument};
    use tempfile::TempDir;

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
        )
        .unwrap(); // Must NOT fail despite provider error

        assert!(!results.results.is_empty(), "should still return keyword results");
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
        )
        .unwrap();

        // relaxed should be false for a simple single-term query that matches
        assert!(!results.relaxed, "relaxed flag should be preserved from tantivy");
    }
}

/// Semantic fallback gate tests for fast_search.
///
/// Verifies that `should_use_semantic_fallback` correctly gates on:
/// - Natural language queries (multi-word, no identifiers)
/// - Sparse keyword results (< 3)
#[cfg(test)]
mod fast_search_fallback_tests {
    use crate::search::hybrid::should_use_semantic_fallback;

    #[test]
    fn test_fallback_triggers_for_nl_with_sparse_results() {
        assert!(should_use_semantic_fallback("how does payment work", 2));
        assert!(should_use_semantic_fallback("what handles authentication", 0));
    }

    #[test]
    fn test_fallback_does_not_trigger_for_identifiers() {
        assert!(!should_use_semantic_fallback("UserService", 5));
        assert!(!should_use_semantic_fallback("process_payment", 10));
    }

    #[test]
    fn test_fallback_does_not_trigger_with_enough_results() {
        assert!(!should_use_semantic_fallback("how does payment work", 5));
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
                    id, name, kind, file_path, language,
                    start_line, start_line + 10,
                    signature, doc_comment
                ],
            )
            .expect("Failed to insert test symbol");
    }

    #[test]
    fn test_knn_to_search_results_converts_correctly() {
        let (mut db, _dir) = create_test_db();

        insert_test_symbol(
            &mut db, "sym1", "process_data", "function", "rust",
            "src/lib.rs", 10, Some("fn process_data(input: &str) -> Result<()>"),
            Some("Processes input data."),
        );
        insert_test_symbol(
            &mut db, "sym2", "UserService", "struct", "rust",
            "src/service.rs", 25, None, None,
        );

        // distance=0.1 → score=0.9, distance=0.3 → score=0.7
        let knn_results = vec![
            ("sym1".to_string(), 0.1_f64),
            ("sym2".to_string(), 0.3_f64),
        ];

        let results = knn_to_search_results(&knn_results, &db).unwrap();

        assert_eq!(results.len(), 2, "should convert both symbols");

        // First result: process_data
        assert_eq!(results[0].id, "sym1");
        assert_eq!(results[0].name, "process_data");
        assert_eq!(results[0].kind, "function");
        assert_eq!(results[0].language, "rust");
        assert_eq!(results[0].file_path, "src/lib.rs");
        assert_eq!(results[0].start_line, 10);
        assert_eq!(results[0].signature, "fn process_data(input: &str) -> Result<()>");
        assert_eq!(results[0].doc_comment, "Processes input data.");
        assert!((results[0].score - 0.9).abs() < 1e-5, "score should be 1.0 - 0.1 = 0.9, got {}", results[0].score);

        // Second result: UserService (no signature/doc_comment → empty strings)
        assert_eq!(results[1].id, "sym2");
        assert_eq!(results[1].name, "UserService");
        assert_eq!(results[1].kind, "struct");
        assert_eq!(results[1].signature, "");
        assert_eq!(results[1].doc_comment, "");
        assert!((results[1].score - 0.7).abs() < 1e-5, "score should be 1.0 - 0.3 = 0.7, got {}", results[1].score);
    }

    #[test]
    fn test_knn_to_search_results_skips_missing_symbols() {
        let (mut db, _dir) = create_test_db();

        insert_test_symbol(
            &mut db, "sym1", "real_function", "function", "python",
            "src/main.py", 1, None, None,
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

        assert!(results.is_empty(), "empty input should produce empty output");
    }
}
