//! Integration tests for get_context pipeline behavior.

#[cfg(test)]
mod pipeline_integration_tests {
    use tempfile::TempDir;

    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
    use crate::search::index::{SearchIndex, SymbolDocument};
    use crate::tools::get_context::pipeline::run_pipeline;

    fn setup_test_env() -> (TempDir, TempDir, SymbolDatabase, SearchIndex) {
        let db_dir = TempDir::new().unwrap();
        let index_dir = TempDir::new().unwrap();

        let db_path = db_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        let index = SearchIndex::create(index_dir.path()).unwrap();

        for file in &[
            "src/handler.rs",
            "src/validation.rs",
            "src/response.rs",
            "src/error.rs",
            "src/main.rs",
        ] {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 1,
                line_count: 0,
                content: None,
            })
            .unwrap();
        }

        let symbols = vec![
            Symbol {
                id: "sym_process".to_string(),
                name: "process_request".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/handler.rs".to_string(),
                start_line: 10,
                end_line: 25,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 500,
                parent_id: None,
                signature: Some("fn process_request(req: &Request) -> Response".to_string()),
                doc_comment: Some("Process an incoming request".to_string()),
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.95),
                code_context: Some(
                    "fn process_request(req: &Request) -> Response {\n    let valid = validate_input(req);\n    if !valid { return Response::bad_request(); }\n    build_response(req)\n}"
                        .to_string(),
                ),
                content_type: None,
            },
            Symbol {
                id: "sym_validate".to_string(),
                name: "validate_input".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/validation.rs".to_string(),
                start_line: 5,
                end_line: 15,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 300,
                parent_id: None,
                signature: Some("fn validate_input(req: &Request) -> bool".to_string()),
                doc_comment: Some("Validate request parameters".to_string()),
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(
                    "fn validate_input(req: &Request) -> bool {\n    !req.body.is_empty()\n}"
                        .to_string(),
                ),
                content_type: None,
            },
            Symbol {
                id: "sym_build".to_string(),
                name: "build_response".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/response.rs".to_string(),
                start_line: 1,
                end_line: 10,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 200,
                parent_id: None,
                signature: Some("fn build_response(req: &Request) -> Response".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(
                    "fn build_response(req: &Request) -> Response {\n    Response::ok(req.body.clone())\n}"
                        .to_string(),
                ),
                content_type: None,
            },
            Symbol {
                id: "sym_error".to_string(),
                name: "handle_error".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/error.rs".to_string(),
                start_line: 1,
                end_line: 8,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 150,
                parent_id: None,
                signature: Some("fn handle_error(err: Error) -> Response".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: None,
                content_type: None,
            },
            Symbol {
                id: "sym_main".to_string(),
                name: "main".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/main.rs".to_string(),
                start_line: 1,
                end_line: 5,
                start_column: 0,
                end_column: 1,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some("fn main()".to_string()),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: None,
                content_type: None,
            },
        ];

        db.store_symbols(&symbols).unwrap();

        for sym in &symbols {
            index.add_symbol(&SymbolDocument::from_symbol(sym)).unwrap();
        }
        index.commit().unwrap();

        let rels = vec![
            Relationship {
                id: "r1".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "sym_validate".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 11,
                confidence: 0.9,
                metadata: None,
            },
            Relationship {
                id: "r2".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "sym_build".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 13,
                confidence: 0.9,
                metadata: None,
            },
            Relationship {
                id: "r3".to_string(),
                from_symbol_id: "sym_error".to_string(),
                to_symbol_id: "sym_process".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/error.rs".to_string(),
                line_number: 3,
                confidence: 0.9,
                metadata: None,
            },
            Relationship {
                id: "r4".to_string(),
                from_symbol_id: "sym_main".to_string(),
                to_symbol_id: "sym_process".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/main.rs".to_string(),
                line_number: 2,
                confidence: 0.9,
                metadata: None,
            },
        ];
        db.store_relationships(&rels).unwrap();
        db.compute_reference_scores().unwrap();

        (db_dir, index_dir, db, index)
    }

    #[test]
    fn test_full_pipeline_end_to_end() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();

        let result =
            run_pipeline("process_request", None, None, None, None, &db, &index, None).unwrap();
        assert!(result.contains("process_request"));
        assert!(result.contains("PIVOT"));
        assert!(result.contains("validate_input"));
        assert!(result.contains("src/handler.rs"));
        assert!(result.contains("Context"));
    }

    #[test]
    fn test_pipeline_no_results() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();
        let result = run_pipeline(
            "zzz_nonexistent_symbol_xyz",
            None,
            None,
            None,
            None,
            &db,
            &index,
            None,
        )
        .unwrap();
        // Default format is compact: "Context ... | no relevant symbols"
        assert!(
            result.contains("no relevant symbols"),
            "Expected no-results message, got:\n{}",
            result
        );
    }

    #[test]
    fn test_pipeline_with_explicit_budget() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();
        let result = run_pipeline(
            "process_request",
            Some(1000),
            None,
            None,
            None,
            &db,
            &index,
            None,
        )
        .unwrap();
        assert!(result.contains("process_request"));
    }

    #[test]
    fn test_pipeline_with_compact_format() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();
        let result = run_pipeline(
            "process_request",
            None,
            None,
            None,
            Some("compact".to_string()),
            &db,
            &index,
            None,
        )
        .unwrap();

        assert!(result.contains("PIVOT process_request"));
        assert!(result.contains("Context \"process_request\" | pivots="));
    }

    #[test]
    fn test_pipeline_with_language_filter() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();
        let result = run_pipeline(
            "process_request",
            None,
            Some("python".to_string()),
            None,
            None,
            &db,
            &index,
            None,
        )
        .unwrap();

        // Default format is compact: "Context ... | no relevant symbols"
        assert!(
            result.contains("no relevant symbols"),
            "Expected no-results message, got:\n{}",
            result
        );
    }

    #[test]
    fn test_pipeline_includes_neighbors() {
        let (_db_dir, _idx_dir, db, index) = setup_test_env();
        let result =
            run_pipeline("process_request", None, None, None, None, &db, &index, None).unwrap();

        let has_neighbor_section = result.contains("Neighbors");
        let has_any_neighbor = result.contains("validate_input")
            || result.contains("build_response")
            || result.contains("handle_error")
            || result.contains("main");
        assert!(has_neighbor_section || has_any_neighbor);
    }

    #[test]
    fn test_pipeline_filters_noise_neighbors() {
        let (_db_dir, _index_dir, mut db, index) = setup_test_env();

        fn make_noise(id: &str, name: &str, file: &str, line: u32) -> Symbol {
            Symbol {
                id: id.to_string(),
                name: name.to_string(),
                kind: SymbolKind::Method,
                language: "rust".to_string(),
                file_path: file.to_string(),
                start_line: line,
                end_line: line + 5,
                start_column: 0,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                parent_id: None,
                signature: Some(format!("fn {}()", name)),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                metadata: None,
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(format!("fn {}() {{ }}", name)),
                content_type: None,
            }
        }

        let noise_symbols = vec![
            make_noise("clone_impl", "clone", "src/handler.rs", 200),
            make_noise("to_string_impl", "to_string", "src/handler.rs", 210),
            make_noise("fmt_impl", "fmt", "src/handler.rs", 220),
        ];

        db.store_symbols(&noise_symbols).unwrap();

        let noise_rels = vec![
            Relationship {
                id: "rel_clone".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "clone_impl".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 55,
                confidence: 0.9,
                metadata: None,
            },
            Relationship {
                id: "rel_to_string".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "to_string_impl".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 56,
                confidence: 0.9,
                metadata: None,
            },
            Relationship {
                id: "rel_fmt".to_string(),
                from_symbol_id: "sym_process".to_string(),
                to_symbol_id: "fmt_impl".to_string(),
                kind: RelationshipKind::Calls,
                file_path: "src/handler.rs".to_string(),
                line_number: 57,
                confidence: 0.9,
                metadata: None,
            },
        ];

        db.store_relationships(&noise_rels).unwrap();

        let result =
            run_pipeline("process_request", None, None, None, None, &db, &index, None).unwrap();

        assert!(!result.contains("clone_impl") && !result.contains("  clone "));
        assert!(!result.contains("to_string_impl") && !result.contains("  to_string "));
        assert!(!result.contains("fmt_impl") && !result.contains("  fmt "));
        assert!(result.contains("validate_input") || result.contains("build_response"));
    }

    #[test]
    fn test_pipeline_filters_test_file_neighbors() {
        let (_db_dir, _index_dir, mut db, index) = setup_test_env();

        db.store_file_info(&FileInfo {
            path: "src/tests/handler_tests.rs".to_string(),
            language: "rust".to_string(),
            hash: "hash_test".to_string(),
            size: 500,
            last_modified: 1000000,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 0,
            content: None,
        })
        .unwrap();

        let test_sym = Symbol {
            id: "test_caller".to_string(),
            name: "test_process_request_works".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/tests/handler_tests.rs".to_string(),
            start_line: 10,
            end_line: 20,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: None,
            signature: Some("fn test_process_request_works()".to_string()),
            doc_comment: None,
            visibility: Some(Visibility::Public),
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: Some(
                "fn test_process_request_works() { process_request(&req); }".to_string(),
            ),
            content_type: None,
        };
        db.store_symbols(&[test_sym]).unwrap();

        let test_rel = Relationship {
            id: "rel_test_caller".to_string(),
            from_symbol_id: "test_caller".to_string(),
            to_symbol_id: "sym_process".to_string(),
            kind: RelationshipKind::Calls,
            file_path: "src/tests/handler_tests.rs".to_string(),
            line_number: 15,
            confidence: 0.9,
            metadata: None,
        };
        db.store_relationships(&[test_rel]).unwrap();

        let result =
            run_pipeline("process_request", None, None, None, None, &db, &index, None).unwrap();

        assert!(!result.contains("test_process_request_works"));
        assert!(
            result.contains("validate_input")
                || result.contains("build_response")
                || result.contains("handle_error")
        );
    }

    #[test]
    fn test_pipeline_respects_token_budget() {
        let (_db_dir, _index_dir, db, index) = setup_test_env();
        let result =
            run_pipeline("process", Some(100), None, None, None, &db, &index, None).unwrap();

        assert!(!result.is_empty());
        let token_est = crate::utils::token_estimation::TokenEstimator::new();
        let estimated = token_est.estimate_string(&result);
        assert!(estimated < 400);
    }
}
