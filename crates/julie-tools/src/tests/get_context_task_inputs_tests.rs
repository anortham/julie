#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use julie_extractors::SymbolKind;
    use julie_facts::rows::{Span, SymbolRow};
    use julie_index::search::index::SymbolSearchResult;

    use crate::get_context::pipeline::run_pipeline_with_options;
    use crate::get_context::scoring::select_pivots_with_task_signals_for_query;
    use crate::get_context::task_signals::{TaskSignals, hydrate_failing_test_links};
    use crate::tests::get_context_tests::snapshot_fixture;

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
            role: String::new(),
            test_role: String::new(),
        }
    }

    fn make_row(
        id: &str,
        name: &str,
        path: &str,
        metadata: Option<serde_json::Value>,
    ) -> SymbolRow {
        SymbolRow {
            id: id.to_string(),
            blob_hash: format!("blob_{id}"),
            ordinal: 0,
            path: path.to_string(),
            language: "rust".to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            span: Span {
                start_line: 1,
                start_col: 0,
                end_line: 6,
                end_col: 0,
                start_byte: 0,
                end_byte: 0,
            },
            body_span: None,
            body_hash: None,
            signature: Some(format!("fn {}()", name)),
            doc_comment: None,
            visibility: None,
            parent_ordinal: None,
            annotations: Vec::new(),
            metadata: metadata.map(|value| serde_json::from_value(value).unwrap()),
            semantic_group: None,
            confidence: None,
            content_type: None,
        }
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
        let (_dir, fixture) = snapshot_fixture(&[
            (
                "src/handler.rs",
                "fn process_request() {\n    validate_input();\n}\n",
            ),
            (
                "src/validation.rs",
                "fn validate_input() {\n    parse_payload();\n}\n",
            ),
            ("src/parser.rs", "fn parse_payload() {\n    true\n}\n"),
        ]);
        let snapshot = fixture.snapshot();
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
            &snapshot,
            None,
            Some(&signals),
        )
        .unwrap();

        assert!(
            output.contains("parse_payload"),
            "second-hop symbol should be included when the first hop is thin: {output}"
        );
        assert!(
            !output.contains("  process_request src/handler.rs:1"),
            "second-hop expansion should not re-add the original pivot as a neighbor: {output}"
        );
    }

    #[test]
    fn test_run_pipeline_with_task_signals_seeds_entry_symbol_missing_from_search_results() {
        let mut files: Vec<(String, String)> = (0..35)
            .map(|idx| {
                (
                    format!("src/noise_{idx}.rs"),
                    format!("fn router_noise_{idx}() {{\n    route_request();\n}}\n"),
                )
            })
            .collect();
        files.push((
            "src/critical.rs".to_string(),
            "fn critical_entry() {\n    handle_critical_path();\n}\n".to_string(),
        ));
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str()))
            .collect();
        let (_dir, fixture) = snapshot_fixture(&borrowed);
        let snapshot = fixture.snapshot();
        let signals = TaskSignals {
            entry_symbols: vec!["crate::critical_entry".to_string()],
            ..TaskSignals::default()
        };

        let output = run_pipeline_with_options(
            "router request",
            None,
            None,
            None,
            Some("readable".to_string()),
            &snapshot,
            None,
            Some(&signals),
        )
        .unwrap();

        assert!(
            output.contains("-- Pivot: critical_entry ---"),
            "entry_symbols should seed named symbols even when query search misses them: {output}"
        );
    }

    #[test]
    fn test_run_pipeline_applies_file_pattern_to_task_seeded_symbols() {
        let (_dir, fixture) = snapshot_fixture(&[
            (
                "src/critical.rs",
                "fn critical_entry() {\n    handle_critical_path();\n}\n",
            ),
            (
                "src/generated/critical.rs",
                "fn critical_entry() {\n    generated_path();\n}\n",
            ),
        ]);
        let snapshot = fixture.snapshot();
        let signals = TaskSignals {
            entry_symbols: vec!["critical_entry".to_string()],
            ..TaskSignals::default()
        };

        let output = run_pipeline_with_options(
            "router request",
            None,
            None,
            Some("src/critical.rs".to_string()),
            Some("readable".to_string()),
            &snapshot,
            None,
            Some(&signals),
        )
        .unwrap();

        assert!(
            output.contains("src/critical.rs:1"),
            "matching task-seeded symbol should survive file_pattern: {output}"
        );
        assert!(
            !output.contains("src/generated/critical.rs"),
            "task-seeded symbols should not bypass file_pattern: {output}"
        );
    }

    #[test]
    fn test_run_pipeline_seeds_edited_file_by_indexed_path_suffix() {
        let (_dir, fixture) = snapshot_fixture(&[(
            "src/critical.rs",
            "fn critical_entry() {\n    handle_critical_path();\n}\n",
        )]);
        let snapshot = fixture.snapshot();
        let signals = TaskSignals {
            edited_files: vec!["critical.rs".to_string()],
            ..TaskSignals::default()
        };

        let output = run_pipeline_with_options(
            "query_without_matches",
            None,
            None,
            None,
            Some("readable".to_string()),
            &snapshot,
            None,
            Some(&signals),
        )
        .unwrap();

        assert!(
            output.contains("-- Pivot: critical_entry ---"),
            "edited_files should seed symbols when the signal is a suffix of the indexed path: {output}"
        );
    }

    #[test]
    fn test_run_pipeline_reports_identifier_callers_in_pivot_summary() {
        let (_dir, fixture) = snapshot_fixture(&[
            (
                "src/pipeline.rs",
                "fn BuildPipeline() {\n    compile_steps();\n}\n",
            ),
            (
                "src/handler.rs",
                "fn setup_handler() {\n    BuildPipeline();\n}\n",
            ),
        ]);
        let snapshot = fixture.snapshot();
        let signals = TaskSignals {
            entry_symbols: vec!["BuildPipeline".to_string()],
            ..TaskSignals::default()
        };

        let output = run_pipeline_with_options(
            "BuildPipeline",
            None,
            None,
            None,
            Some("readable".to_string()),
            &snapshot,
            None,
            Some(&signals),
        )
        .unwrap();

        assert!(
            output.contains("Callers (1): setup_handler"),
            "identifier-only callers should appear in the pivot caller summary: {output}"
        );
    }

    #[test]
    fn test_run_pipeline_with_task_signals_does_not_append_a_paging_trailer() {
        let calls: String = (0..24)
            .map(|idx| format!("    validate_{idx}();\n"))
            .collect();
        let mut files: Vec<(String, String)> = vec![(
            "src/handler.rs".to_string(),
            format!("fn process_request() {{\n{calls}}}\n"),
        )];
        for idx in 0..24 {
            files.push((
                format!("src/validation_{idx}.rs"),
                format!(
                    "fn validate_{idx}() {{\n    // {}\n    // {}\n    // {}\n}}\n",
                    "x".repeat(120),
                    "y".repeat(120),
                    "z".repeat(120)
                ),
            ));
        }
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, content)| (path.as_str(), content.as_str()))
            .collect();
        let (_dir, fixture) = snapshot_fixture(&borrowed);
        let snapshot = fixture.snapshot();
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
            &snapshot,
            None,
            Some(&signals),
        )
        .unwrap();

        assert!(
            !output.contains("Output truncated"),
            "get_context is budgeted, not paged: {output}"
        );
        assert!(!output.contains("next:"), "{output}");
    }

    #[test]
    fn test_hydrate_failing_test_links_matches_linked_test_paths() {
        let payment = make_row(
            "payment",
            "process_payment",
            "src/payment.rs",
            Some(serde_json::json!({
                "test_linkage": {
                    "test_count": 1,
                    "best_tier": "thorough",
                    "worst_tier": "thorough",
                    "linked_tests": ["test_process_payment"],
                    "linked_test_paths": ["tests/payment_service_tests.rs"],
                    "evidence_sources": ["relationship"]
                }
            })),
        );
        let helper = make_row("helper", "render_invoice", "src/invoice.rs", None);
        let mut signals = TaskSignals {
            failing_test: Some("tests/payment_service_tests.rs".to_string()),
            ..TaskSignals::default()
        };

        hydrate_failing_test_links([&payment, &helper], &mut signals);

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
        let unrelated = make_row(
            "unrelated",
            "unrelated_symbol",
            "src/unrelated.rs",
            Some(serde_json::json!({
                "test_linkage": {
                    "test_count": 1,
                    "best_tier": "thorough",
                    "worst_tier": "thorough",
                    "linked_tests": ["something_else"],
                    "linked_test_paths": ["tests/paymentXserviceXtests.rs"],
                    "evidence_sources": ["relationship"]
                }
            })),
        );
        let mut signals = TaskSignals {
            failing_test: Some("tests/payment_service_tests.rs".to_string()),
            ..TaskSignals::default()
        };

        hydrate_failing_test_links([&unrelated], &mut signals);

        assert!(
            !signals.failing_test_linked_symbol_ids.contains("unrelated"),
            "underscores in the failing-test path must be literal, not LIKE wildcards"
        );
    }
}
