#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tempfile::TempDir;

    use crate::database::{FileInfo, SymbolDatabase};
    use crate::search::index::{SearchIndex, SymbolDocument, SymbolSearchResult};
    use crate::tools::get_context::pipeline::run_pipeline_with_options;
    use crate::tools::get_context::scoring::select_pivots_with_task_signals_for_query;
    use crate::tools::get_context::task_signals::{TaskSignals, hydrate_failing_test_links};
    use crate::tools::spillover::store::SpilloverStore;
    use crate::{
        extractors::base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility},
        tools::spillover::SpilloverFormat,
    };

    fn make_result(id: &str, name: &str, file_path: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: format!("fn {}()", name),
            doc_comment: String::new(),
            file_path: file_path.to_string(),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        }
    }

    fn make_file(path: &str) -> FileInfo {
        FileInfo {
            path: path.to_string(),
            language: "rust".to_string(),
            hash: format!("hash_{path}"),
            size: 512,
            last_modified: 1_700_000_000,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 20,
            content: None,
        }
    }

    fn make_symbol(id: &str, name: &str, file_path: &str, body: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            end_line: 6,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: body.len() as u32,
            parent_id: None,
            signature: Some(format!("fn {}()", name)),
            doc_comment: Some(format!("{} docs", name)),
            visibility: Some(Visibility::Public),
            metadata: None,
            semantic_group: None,
            confidence: Some(1.0),
            code_context: Some(body.to_string()),
            content_type: None,
            annotations: Vec::new(),
        }
    }

    fn make_relationship(
        id: &str,
        from_symbol_id: &str,
        to_symbol_id: &str,
        file_path: &str,
    ) -> Relationship {
        Relationship {
            id: id.to_string(),
            from_symbol_id: from_symbol_id.to_string(),
            to_symbol_id: to_symbol_id.to_string(),
            kind: RelationshipKind::Calls,
            file_path: file_path.to_string(),
            line_number: 2,
            confidence: 1.0,
            metadata: None,
        }
    }

    fn setup_env(
        symbols: &[Symbol],
        relationships: &[Relationship],
    ) -> (TempDir, TempDir, SymbolDatabase, SearchIndex) {
        let db_dir = TempDir::new().unwrap();
        let index_dir = TempDir::new().unwrap();
        let db_path = db_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        let index = SearchIndex::create(index_dir.path()).unwrap();

        let mut seen_files = HashMap::new();
        for symbol in symbols {
            seen_files
                .entry(symbol.file_path.clone())
                .or_insert_with(|| make_file(&symbol.file_path));
        }
        for file in seen_files.into_values() {
            db.store_file_info(&file).unwrap();
        }

        db.store_symbols(symbols).unwrap();
        db.store_relationships(relationships).unwrap();
        db.compute_reference_scores().unwrap();

        for symbol in symbols {
            index
                .add_symbol(&SymbolDocument::from_symbol(symbol))
                .unwrap();
        }
        index.commit().unwrap();

        (db_dir, index_dir, db, index)
    }

    #[test]
    fn test_select_pivots_with_task_signals_boosts_edited_file_and_entry_symbol() {
        let results = vec![
            make_result("higher", "other_handler", "src/other.rs", 5.0),
            make_result("target", "refresh_token", "src/auth.rs", 2.0),
        ];
        let ref_scores = HashMap::new();
        let signals = TaskSignals {
            edited_files: vec!["src/auth.rs".to_string()],
            entry_symbols: vec!["AuthService::refresh_token".to_string()],
            ..TaskSignals::default()
        };

        let pivots = select_pivots_with_task_signals_for_query(
            "token refresh",
            results,
            &ref_scores,
            &signals,
        );

        assert!(!pivots.is_empty(), "expected boosted pivots");
        assert_eq!(
            pivots[0].result.name, "refresh_token",
            "task-shaped boosts should surface the edited entry symbol"
        );
    }

    #[test]
    fn test_run_pipeline_with_task_signals_adds_second_hop_when_requested() {
        let symbols = vec![
            make_symbol(
                "process",
                "process_request",
                "src/handler.rs",
                "fn process_request() {\n    validate_input();\n}",
            ),
            make_symbol(
                "validate",
                "validate_input",
                "src/validation.rs",
                "fn validate_input() {\n    parse_payload();\n}",
            ),
            make_symbol(
                "parse",
                "parse_payload",
                "src/parser.rs",
                "fn parse_payload() {\n    true\n}",
            ),
        ];
        let relationships = vec![
            make_relationship("r1", "process", "validate", "src/handler.rs"),
            make_relationship("r2", "validate", "parse", "src/validation.rs"),
        ];
        let (_db_dir, _index_dir, db, index) = setup_env(&symbols, &relationships);
        let signals = TaskSignals {
            entry_symbols: vec!["process_request".to_string()],
            max_hops: 2,
            ..TaskSignals::default()
        };

        let output = run_pipeline_with_options(
            "process_request",
            None,
            None,
            None,
            Some("readable".to_string()),
            &db,
            &index,
            None,
            Some(&signals),
            None,
            None,
        )
        .unwrap();

        assert!(
            output.contains("parse_payload"),
            "second-hop symbol should be included when the first hop is thin: {output}"
        );
    }

    #[test]
    fn test_run_pipeline_with_task_signals_emits_spillover_handle_for_overflow_neighbors() {
        let mut symbols = vec![make_symbol(
            "pivot",
            "process_request",
            "src/handler.rs",
            "fn process_request() {\n    validate_0();\n}",
        )];
        let mut relationships = Vec::new();

        for idx in 0..24 {
            let name = format!("validate_{}", idx);
            let symbol_id = format!("neighbor_{}", idx);
            let long_body = format!(
                "fn {}() {{\n    // {}\n    // {}\n    // {}\n}}",
                name,
                "x".repeat(120),
                "y".repeat(120),
                "z".repeat(120)
            );
            symbols.push(make_symbol(
                &symbol_id,
                &name,
                &format!("src/validation_{}.rs", idx),
                &long_body,
            ));
            relationships.push(make_relationship(
                &format!("r{}", idx),
                "pivot",
                &symbol_id,
                "src/handler.rs",
            ));
        }

        let (_db_dir, _index_dir, db, index) = setup_env(&symbols, &relationships);
        let spillover_store = SpilloverStore::new(32, std::time::Duration::from_secs(60));
        let signals = TaskSignals {
            entry_symbols: vec!["process_request".to_string()],
            max_hops: 1,
            ..TaskSignals::default()
        };

        let output = run_pipeline_with_options(
            "process_request",
            Some(200),
            None,
            None,
            Some("readable".to_string()),
            &db,
            &index,
            None,
            Some(&signals),
            Some(&spillover_store),
            Some(("session-a", SpilloverFormat::Readable)),
        )
        .unwrap();

        assert!(
            output.contains("More available: spillover_handle=gc_"),
            "neighbor overflow should spill into follow-up pages: {output}"
        );
        assert!(
            output.contains("Next page: spillover_get(spillover_handle=\"gc_"),
            "neighbor overflow should include a paging call hint: {output}"
        );
    }

    #[test]
    fn test_hydrate_failing_test_links_matches_linked_test_paths() {
        let mut payment = make_symbol(
            "payment",
            "process_payment",
            "src/payment.rs",
            "fn process_payment() {\n    true\n}",
        );
        payment.metadata = Some(
            serde_json::from_value(serde_json::json!({
                "test_linkage": {
                    "test_count": 1,
                    "best_tier": "thorough",
                    "worst_tier": "thorough",
                    "linked_tests": ["test_process_payment"],
                    "linked_test_paths": ["tests/payment_service_tests.rs"],
                    "evidence_sources": ["relationship"]
                }
            }))
            .unwrap(),
        );

        let helper = make_symbol(
            "helper",
            "render_invoice",
            "src/invoice.rs",
            "fn render_invoice() {\n    true\n}",
        );

        let (_db_dir, _index_dir, db, _index) = setup_env(&[payment, helper], &[]);
        let mut signals = TaskSignals {
            failing_test: Some("tests/payment_service_tests.rs".to_string()),
            ..TaskSignals::default()
        };

        hydrate_failing_test_links(&db, &mut signals).unwrap();

        assert!(
            signals.failing_test_linked_symbol_ids.contains("payment"),
            "linked production symbol should be hydrated from linked_test_paths"
        );
        assert!(
            !signals.failing_test_linked_symbol_ids.contains("helper"),
            "unlinked symbols should not be hydrated"
        );
    }

    #[test]
    fn test_hydrate_failing_test_links_treats_underscores_as_literals() {
        let mut unrelated = make_symbol(
            "unrelated",
            "unrelated_symbol",
            "src/unrelated.rs",
            "fn unrelated_symbol() {}",
        );
        // Path uses literal 'X' chars at the positions where the failing test
        // path has '_'. Without ESCAPE on the LIKE patterns, SQLite treats the
        // '_' in the failing-test parameter as a single-char wildcard and the
        // 'X's would match it — a false positive. The escape pass should make
        // the underscore a literal so this row stays unmatched.
        unrelated.metadata = Some(
            serde_json::from_value(serde_json::json!({
                "test_linkage": {
                    "test_count": 1,
                    "best_tier": "thorough",
                    "worst_tier": "thorough",
                    "linked_tests": ["something_else"],
                    "linked_test_paths": ["tests/paymentXserviceXtests.rs"],
                    "evidence_sources": ["relationship"]
                }
            }))
            .unwrap(),
        );

        let (_db_dir, _index_dir, db, _index) = setup_env(&[unrelated], &[]);
        let mut signals = TaskSignals {
            failing_test: Some("tests/payment_service_tests.rs".to_string()),
            ..TaskSignals::default()
        };

        hydrate_failing_test_links(&db, &mut signals).unwrap();

        assert!(
            !signals.failing_test_linked_symbol_ids.contains("unrelated"),
            "underscores in the failing-test path must be literal, not LIKE wildcards"
        );
    }
}
