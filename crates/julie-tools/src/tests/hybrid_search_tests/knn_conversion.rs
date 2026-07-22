/// KNN-to-SymbolSearchResult conversion tests.
///
/// Verifies that `knn_to_search_results` correctly converts (symbol_id, distance)
/// pairs from sqlite-vec into SymbolSearchResult objects by looking up metadata
/// from the database.
#[cfg(test)]
mod conversion_tests {
    use std::collections::HashMap;

    use julie_core::database::SymbolDatabase;
    use julie_extractors::SymbolKind;
    use julie_index::search::hybrid::knn_to_search_results;
    use julie_test_support::db::{file_info_builder, store_file_info_if_missing, symbol_builder};
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
        insert_test_symbol_with_metadata(
            db,
            id,
            name,
            kind,
            language,
            file_path,
            start_line,
            signature,
            doc_comment,
            None,
        );
    }

    fn insert_test_symbol_with_metadata(
        db: &mut SymbolDatabase,
        id: &str,
        name: &str,
        kind: &str,
        language: &str,
        file_path: &str,
        start_line: u32,
        signature: Option<&str>,
        doc_comment: Option<&str>,
        metadata: Option<&str>,
    ) {
        store_file_info_if_missing(
            db,
            &file_info_builder(file_path)
                .language(language)
                .hash("deadbeef")
                .size(100)
                .last_modified(0)
                .last_indexed(0)
                .symbol_count(0)
                .line_count(0)
                .build(),
        )
        .expect("Failed to insert test file");

        let mut symbol = symbol_builder(id, name, file_path)
            .kind(SymbolKind::from_string(kind))
            .language(language)
            .span(start_line, 0, start_line + 10, 0)
            .bytes(0, 100)
            .confidence(1.0);

        if let Some(signature) = signature {
            symbol = symbol.signature(signature);
        }
        if let Some(doc_comment) = doc_comment {
            symbol = symbol.doc_comment(doc_comment);
        }
        if let Some(metadata) = metadata {
            let metadata: HashMap<String, serde_json::Value> =
                serde_json::from_str(metadata).expect("Failed to parse test metadata");
            symbol = symbol.metadata(metadata);
        }

        db.store_symbols(&[symbol.build()])
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
    fn test_knn_to_search_results_uses_metadata_role_for_inline_tests() {
        let (mut db, _dir) = create_test_db();
        insert_test_symbol_with_metadata(
            &mut db,
            "inline_test_sym",
            "inline_test_in_production_path",
            "function",
            "rust",
            "src/lib.rs",
            30,
            Some("fn inline_test_in_production_path()"),
            None,
            Some(r#"{"is_test":true,"test_role":"impl_test"}"#),
        );

        let knn_results = vec![("inline_test_sym".to_string(), 0.05_f64)];
        let results = knn_to_search_results(&knn_results, &db).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].role, "test");
        assert_eq!(results[0].test_role, "impl_test");
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

    // Regression: distance > 1.0 (e.g. cosine distance on un-normalized vectors)
    // should clamp to 0.0, not produce a negative score.
    #[test]
    fn test_knn_score_clamp_negative_distance() {
        let (mut db, _dir) = create_test_db();
        insert_test_symbol(
            &mut db,
            "sym_clamp",
            "clamp_me",
            "function",
            "rust",
            "src/clamp.rs",
            1,
            None,
            None,
        );

        let knn_results = vec![("sym_clamp".to_string(), 1.5f64)];
        let results = knn_to_search_results(&knn_results, &db).unwrap();

        assert_eq!(results.len(), 1);
        assert!(
            results[0].score >= 0.0,
            "score must be non-negative, got {}",
            results[0].score
        );
        assert_eq!(
            results[0].score, 0.0,
            "distance=1.5 should clamp to score=0.0, got {}",
            results[0].score
        );
    }
}
