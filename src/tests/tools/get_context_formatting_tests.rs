//! Tests for get_context output formatting — rendering pivots, neighbors, file map.

#[cfg(test)]
mod formatting_tests {
    use crate::utils::token_estimation::TokenEstimator;

    use crate::tools::get_context::allocation::{Allocation, NeighborMode, PivotMode};
    use crate::tools::get_context::formatting::{
        ContextData, NeighborEntry, OutputFormat, PivotEntry, format_context,
        format_context_with_mode,
    };

    /// Helper: build an Allocation with specific modes (token counts don't matter for formatting).
    fn make_allocation(pivot_mode: PivotMode, neighbor_mode: NeighborMode) -> Allocation {
        Allocation {
            pivot_tokens: 1200,
            neighbor_tokens: 600,
            summary_tokens: 200,
            pivot_mode,
            neighbor_mode,
        }
    }

    /// Helper: build a PivotEntry with sensible defaults.
    fn make_pivot(name: &str, file: &str, line: u32, ref_score: f64, content: &str) -> PivotEntry {
        PivotEntry {
            name: name.to_string(),
            file_path: file.to_string(),
            start_line: line,
            kind: "function".to_string(),
            reference_score: ref_score,
            content: content.to_string(),
            incoming_names: vec![],
            outgoing_names: vec![],
            test_quality_label: None,
        }
    }

    /// Helper: build a NeighborEntry.
    fn make_neighbor(
        name: &str,
        file: &str,
        line: u32,
        signature: Option<&str>,
        doc: Option<&str>,
    ) -> NeighborEntry {
        NeighborEntry {
            name: name.to_string(),
            file_path: file.to_string(),
            start_line: line,
            kind: "function".to_string(),
            signature: signature.map(|s| s.to_string()),
            doc_summary: doc.map(|s| s.to_string()),
        }
    }

    // === Test 1: Empty results produces "No relevant symbols found" ===

    #[test]
    fn test_empty_results() {
        let data = ContextData {
            query: "payment processing".to_string(),
            pivots: vec![],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            output.contains("No relevant symbols found"),
            "empty pivots should produce 'No relevant symbols found', got:\n{}",
            output
        );
        assert!(
            output.contains("payment processing"),
            "should include the query in empty output"
        );
    }

    // === Test 2: Output contains all major sections ===

    #[test]
    fn test_contains_all_sections() {
        let data = ContextData {
            query: "payment processing".to_string(),
            pivots: vec![make_pivot(
                "process_payment",
                "src/payment/processor.rs",
                42,
                25.0,
                "pub fn process_payment() { ... }",
            )],
            neighbors: vec![make_neighbor(
                "validate_payment",
                "src/payment/validation.rs",
                10,
                Some("fn validate_payment(...)"),
                Some("Validates payment data"),
            )],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);

        // Header section
        assert!(
            output.contains("Context:"),
            "should have header with Context:"
        );
        assert!(
            output.contains("payment processing"),
            "header should contain query"
        );
        assert!(
            output.contains("1 pivot"),
            "header should show pivot count, got:\n{}",
            output
        );

        // Pivot section
        assert!(
            output.contains("Pivot: process_payment"),
            "should have pivot section"
        );

        // Neighbors section
        assert!(
            output.contains("Neighbors"),
            "should have Neighbors section"
        );
    }

    // === Test 3: Centrality hints ===

    #[test]
    fn test_centrality_high() {
        let data = ContextData {
            query: "test".to_string(),
            pivots: vec![make_pivot(
                "high_fn",
                "src/a.rs",
                1,
                25.0,
                "fn high_fn() {}",
            )],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };
        let output = format_context(&data);
        assert!(
            output.contains("Centrality: high"),
            "ref_score 25 should be 'high', got:\n{}",
            output
        );
    }

    #[test]
    fn test_centrality_medium() {
        let data = ContextData {
            query: "test".to_string(),
            pivots: vec![make_pivot("med_fn", "src/b.rs", 1, 10.0, "fn med_fn() {}")],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };
        let output = format_context(&data);
        assert!(
            output.contains("Centrality: medium"),
            "ref_score 10 should be 'medium', got:\n{}",
            output
        );
    }

    #[test]
    fn test_centrality_low() {
        let data = ContextData {
            query: "test".to_string(),
            pivots: vec![make_pivot("low_fn", "src/c.rs", 1, 2.0, "fn low_fn() {}")],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };
        let output = format_context(&data);
        assert!(
            output.contains("Centrality: low"),
            "ref_score 2 should be 'low', got:\n{}",
            output
        );
    }

    #[test]
    fn test_centrality_boundary_at_20() {
        let data = ContextData {
            query: "test".to_string(),
            pivots: vec![make_pivot(
                "edge_fn",
                "src/d.rs",
                1,
                20.0,
                "fn edge_fn() {}",
            )],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };
        let output = format_context(&data);
        assert!(
            output.contains("Centrality: high"),
            "ref_score exactly 20 should be 'high'"
        );
    }

    #[test]
    fn test_centrality_boundary_at_5() {
        let data = ContextData {
            query: "test".to_string(),
            pivots: vec![make_pivot("edge_fn", "src/e.rs", 1, 5.0, "fn edge_fn() {}")],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };
        let output = format_context(&data);
        assert!(
            output.contains("Centrality: medium"),
            "ref_score exactly 5 should be 'medium'"
        );
    }

    // === Test 4: Pivot content is rendered ===

    #[test]
    fn test_pivot_content_rendered() {
        let code = "pub async fn process_payment(order: &Order) -> Result<Receipt> {\n    validate(order)?;\n    charge(order)\n}";
        let data = ContextData {
            query: "payment".to_string(),
            pivots: vec![make_pivot(
                "process_payment",
                "src/payment.rs",
                42,
                30.0,
                code,
            )],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            output.contains("pub async fn process_payment"),
            "pivot code body should appear in output"
        );
        assert!(
            output.contains("validate(order)?"),
            "pivot code body lines should appear"
        );
        assert!(
            output.contains("charge(order)"),
            "all code lines should appear"
        );
    }

    // === Test 5: Pivot callers and callees ===

    #[test]
    fn test_pivot_callers_and_callees() {
        let mut pivot = make_pivot(
            "engine_run",
            "src/engine.rs",
            10,
            15.0,
            "fn engine_run() {}",
        );
        pivot.incoming_names = vec![
            "main".to_string(),
            "retry_handler".to_string(),
            "batch_processor".to_string(),
        ];
        pivot.outgoing_names = vec!["validate".to_string(), "execute".to_string()];

        let data = ContextData {
            query: "engine".to_string(),
            pivots: vec![pivot],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            output.contains("Callers (3)"),
            "should show caller count, got:\n{}",
            output
        );
        assert!(output.contains("main"), "should list caller names");
        assert!(output.contains("retry_handler"), "should list all callers");
        assert!(
            output.contains("Calls: validate, execute")
                || output.contains("Calls: execute, validate"),
            "should show callee names, got:\n{}",
            output
        );
    }

    #[test]
    fn test_pivot_no_callers_no_calls_line() {
        let pivot = make_pivot("lonely_fn", "src/lonely.rs", 1, 0.5, "fn lonely_fn() {}");
        let data = ContextData {
            query: "lonely".to_string(),
            pivots: vec![pivot],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            !output.contains("Callers"),
            "should not show Callers line when empty"
        );
        assert!(
            !output.contains("Calls:"),
            "should not show Calls line when empty"
        );
    }

    #[test]
    fn test_pivot_callers_and_calls_are_deduplicated() {
        let mut pivot = make_pivot(
            "engine_run",
            "src/engine.rs",
            10,
            15.0,
            "fn engine_run() {}",
        );
        pivot.incoming_names = vec!["main".to_string(), "main".to_string(), "worker".to_string()];
        pivot.outgoing_names = vec![
            "validate".to_string(),
            "validate".to_string(),
            "execute".to_string(),
        ];

        let data = ContextData {
            query: "engine".to_string(),
            pivots: vec![pivot],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            output.contains("Callers (2): main, worker")
                || output.contains("Callers (2): worker, main"),
            "caller names should be deduplicated, got:\n{}",
            output
        );
        assert!(
            output.contains("Calls: execute, validate")
                || output.contains("Calls: validate, execute"),
            "callee names should be deduplicated, got:\n{}",
            output
        );
    }

    // === Test 6: Neighbors rendered (SignatureAndDoc mode) ===

    #[test]
    fn test_neighbors_signature_and_doc_mode() {
        let data = ContextData {
            query: "payment".to_string(),
            pivots: vec![make_pivot(
                "process",
                "src/proc.rs",
                1,
                10.0,
                "fn process() {}",
            )],
            neighbors: vec![
                make_neighbor(
                    "validate_payment",
                    "src/validation.rs",
                    10,
                    Some("fn validate_payment(order: &Order) -> bool"),
                    Some("Validates payment data before processing"),
                ),
                make_neighbor(
                    "Receipt",
                    "src/types.rs",
                    35,
                    Some("pub struct Receipt { ... }"),
                    None,
                ),
            ],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            output.contains("validate_payment"),
            "neighbor name should appear"
        );
        assert!(
            output.contains("src/validation.rs:10"),
            "neighbor location should appear"
        );
        assert!(
            output.contains("fn validate_payment(order: &Order) -> bool"),
            "neighbor signature should appear in SignatureAndDoc mode"
        );
        assert!(
            output.contains("Validates payment data before processing"),
            "neighbor doc summary should appear in SignatureAndDoc mode"
        );
        assert!(output.contains("Receipt"), "second neighbor should appear");
    }

    // === Test 7: NeighborMode::SignatureOnly ===

    #[test]
    fn test_neighbors_signature_only_mode() {
        let data = ContextData {
            query: "engine".to_string(),
            pivots: vec![make_pivot("run", "src/engine.rs", 1, 10.0, "fn run() {}")],
            neighbors: vec![make_neighbor(
                "helper",
                "src/utils.rs",
                5,
                Some("fn helper(x: i32) -> String"),
                Some("This doc should NOT appear in SignatureOnly mode"),
            )],
            allocation: make_allocation(PivotMode::SignatureAndKey, NeighborMode::SignatureOnly),
        };

        let output = format_context(&data);
        assert!(output.contains("helper"), "neighbor name should appear");
        assert!(
            output.contains("fn helper(x: i32) -> String"),
            "signature should appear in SignatureOnly mode"
        );
        assert!(
            !output.contains("This doc should NOT appear"),
            "doc summary should NOT appear in SignatureOnly mode"
        );
    }

    // === Test 8: NeighborMode::NameAndLocation ===

    #[test]
    fn test_neighbors_name_and_location_mode() {
        let data = ContextData {
            query: "broad search".to_string(),
            pivots: vec![make_pivot("main", "src/main.rs", 1, 5.0, "fn main() {}")],
            neighbors: vec![make_neighbor(
                "helper",
                "src/utils.rs",
                5,
                Some("fn helper(x: i32) -> String"),
                Some("Helper doc"),
            )],
            allocation: make_allocation(PivotMode::SignatureOnly, NeighborMode::NameAndLocation),
        };

        let output = format_context(&data);
        assert!(output.contains("helper"), "neighbor name should appear");
        assert!(
            output.contains("src/utils.rs:5"),
            "neighbor location should appear"
        );
        assert!(
            !output.contains("fn helper(x: i32)"),
            "signature should NOT appear in NameAndLocation mode, got:\n{}",
            output
        );
        assert!(
            !output.contains("Helper doc"),
            "doc should NOT appear in NameAndLocation mode"
        );
    }

    // === Test 10: Multiple pivots in output ===

    #[test]
    fn test_multiple_pivots() {
        let data = ContextData {
            query: "engine".to_string(),
            pivots: vec![
                make_pivot(
                    "start_engine",
                    "src/engine.rs",
                    10,
                    20.0,
                    "fn start_engine() {}",
                ),
                make_pivot(
                    "stop_engine",
                    "src/engine.rs",
                    50,
                    15.0,
                    "fn stop_engine() {}",
                ),
            ],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            output.contains("Pivot: start_engine"),
            "first pivot should appear"
        );
        assert!(
            output.contains("Pivot: stop_engine"),
            "second pivot should appear"
        );
        assert!(
            output.contains("2 pivots"),
            "header should show correct count"
        );
    }

    // === Test 11: Pivot location line ===

    #[test]
    fn test_pivot_location_line() {
        let data = ContextData {
            query: "test".to_string(),
            pivots: vec![make_pivot(
                "my_func",
                "src/deep/nested/module.rs",
                142,
                3.0,
                "fn my_func() {}",
            )],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            output.contains("src/deep/nested/module.rs:142"),
            "should show file:line location"
        );
        assert!(
            output.contains("(function)"),
            "should show kind in parentheses"
        );
    }

    // === Test 12: Header summary counts ===

    #[test]
    fn test_header_summary_counts() {
        let data = ContextData {
            query: "search query".to_string(),
            pivots: vec![
                make_pivot("a", "src/a.rs", 1, 1.0, "fn a() {}"),
                make_pivot("b", "src/b.rs", 1, 1.0, "fn b() {}"),
                make_pivot("c", "src/a.rs", 10, 1.0, "fn c() {}"),
            ],
            neighbors: vec![
                make_neighbor("x", "src/x.rs", 1, None, None),
                make_neighbor("y", "src/y.rs", 1, None, None),
            ],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(output.contains("3 pivots"), "should show 3 pivots");
        assert!(output.contains("2 neighbors"), "should show 2 neighbors");
        // Unique files: src/a.rs, src/b.rs, src/x.rs, src/y.rs = 4
        assert!(
            output.contains("4 files"),
            "should show 4 unique files, got:\n{}",
            output
        );
    }

    // === Test 13: Singular counts (1 pivot, 1 neighbor, 1 file) ===

    #[test]
    fn test_singular_counts() {
        let data = ContextData {
            query: "test".to_string(),
            pivots: vec![make_pivot(
                "only_one",
                "src/only.rs",
                1,
                1.0,
                "fn only_one() {}",
            )],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            output.contains("1 pivot,"),
            "should use singular 'pivot' not 'pivots', got:\n{}",
            output
        );
        assert!(output.contains("0 neighbors"), "should show 0 neighbors");
        assert!(
            output.contains("1 file"),
            "should use singular 'file' not 'files'"
        );
    }

    // === Test 14: Centrality label only (no raw ref_score) ===

    #[test]
    fn test_centrality_shown_as_label_only() {
        let data = ContextData {
            query: "test".to_string(),
            pivots: vec![make_pivot("fn_a", "src/a.rs", 1, 47.8, "fn fn_a() {}")],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context(&data);
        assert!(
            output.contains("Centrality: high"),
            "ref_score 47.8 should show 'high' label, got:\n{}",
            output
        );
        assert!(
            !output.contains("ref_score"),
            "raw ref_score should not appear in output, got:\n{}",
            output
        );
    }

    #[test]
    fn test_compact_format_is_token_lean_and_structured() {
        let mut pivot = make_pivot(
            "process_payment",
            "src/payment/processor.rs",
            42,
            25.0,
            "pub fn process_payment() { ... }",
        );
        pivot.incoming_names = vec!["main".to_string(), "main".to_string()];
        pivot.outgoing_names = vec!["validate".to_string(), "validate".to_string()];

        let data = ContextData {
            query: "payment processing".to_string(),
            pivots: vec![pivot],
            neighbors: vec![make_neighbor(
                "validate_payment",
                "src/payment/validation.rs",
                10,
                Some("fn validate_payment(...)"),
                Some("Validates payment data"),
            )],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context_with_mode(&data, OutputFormat::Compact);

        assert!(output.contains("Context \"payment processing\" | pivots=1 neighbors=1 files=2"));
        assert!(output.contains("PIVOT process_payment src/payment/processor.rs:42"));
        assert!(output.contains("NEIGHBOR validate_payment src/payment/validation.rs:10"));
        assert!(
            output.contains("callers=main"),
            "caller list should be deduplicated in compact mode"
        );
        assert!(
            output.contains("calls=validate"),
            "callee list should be deduplicated in compact mode"
        );
        assert!(
            !output.contains("═══"),
            "compact mode should avoid heavy unicode separators"
        );
    }

    #[test]
    fn test_compact_neighbor_mode_name_and_location_omits_signature() {
        let data = ContextData {
            query: "broad search".to_string(),
            pivots: vec![make_pivot("main", "src/main.rs", 1, 5.0, "fn main() {}")],
            neighbors: vec![make_neighbor(
                "helper",
                "src/utils.rs",
                5,
                Some("fn helper(x: i32) -> String"),
                Some("Helper doc"),
            )],
            allocation: make_allocation(PivotMode::SignatureOnly, NeighborMode::NameAndLocation),
        };

        let output = format_context_with_mode(&data, OutputFormat::Compact);
        assert!(output.contains("NEIGHBOR helper src/utils.rs:5 kind=function"));
        assert!(
            !output.contains("sig=fn helper"),
            "NameAndLocation compact mode should omit signatures"
        );
        assert!(
            !output.contains("doc=Helper doc"),
            "NameAndLocation compact mode should omit doc summaries"
        );
    }

    #[test]
    fn test_compact_output_smaller_than_readable_for_same_context() {
        let mut pivot = make_pivot(
            "process_payment",
            "src/payment/processor.rs",
            42,
            25.0,
            "pub fn process_payment(order: &Order) -> Result<Receipt> { validate(order)?; charge(order) }",
        );
        pivot.incoming_names = vec!["api_entry".to_string(), "worker".to_string()];
        pivot.outgoing_names = vec!["validate".to_string(), "charge".to_string()];

        let data = ContextData {
            query: "payment processing".to_string(),
            pivots: vec![pivot],
            neighbors: vec![
                make_neighbor(
                    "validate",
                    "src/payment/validation.rs",
                    10,
                    Some("fn validate(order: &Order) -> Result<()>"),
                    Some("Validates order state"),
                ),
                make_neighbor(
                    "charge",
                    "src/payment/gateway.rs",
                    20,
                    Some("fn charge(order: &Order) -> Result<ChargeId>"),
                    Some("Performs gateway charge"),
                ),
            ],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let readable = format_context(&data);
        let compact = format_context_with_mode(&data, OutputFormat::Compact);

        assert!(!compact.is_empty(), "compact output should not be empty");
        assert!(!readable.is_empty(), "readable output should not be empty");
        let ratio = compact.len() as f64 / readable.len() as f64;
        assert!(
            ratio < 0.90,
            "compact should be at least 10% smaller than readable (compact={}, readable={}, ratio={:.2})",
            compact.len(),
            readable.len(),
            ratio
        );
    }

    #[test]
    fn test_compact_reduces_estimated_tokens_by_at_least_20_percent() {
        let mut pivots = vec![];
        for i in 0..3 {
            let mut pivot = make_pivot(
                &format!("process_batch_{}", i),
                &format!("src/payment/batch_{}.rs", i),
                20 + i,
                30.0 - i as f64,
                "pub fn process_batch(order: &Order) -> Result<Receipt> { validate(order)?; charge(order)?; persist(order) }",
            );
            pivot.incoming_names = vec![
                "api_entry".to_string(),
                "worker".to_string(),
                "worker".to_string(),
            ];
            pivot.outgoing_names = vec![
                "validate".to_string(),
                "charge".to_string(),
                "persist".to_string(),
                "charge".to_string(),
            ];
            pivots.push(pivot);
        }

        let neighbors = vec![
            make_neighbor(
                "validate",
                "src/payment/validation.rs",
                10,
                Some("fn validate(order: &Order) -> Result<()>"),
                Some("Validates order state before charging"),
            ),
            make_neighbor(
                "charge",
                "src/payment/gateway.rs",
                22,
                Some("fn charge(order: &Order) -> Result<ChargeId>"),
                Some("Calls gateway and returns charge id"),
            ),
            make_neighbor(
                "persist",
                "src/payment/store.rs",
                35,
                Some("fn persist(order: &Order) -> Result<()>"),
                Some("Stores payment state transitions"),
            ),
            make_neighbor(
                "audit",
                "src/payment/audit.rs",
                44,
                Some("fn audit(event: AuditEvent)"),
                Some("Writes audit trail event"),
            ),
        ];

        let data = ContextData {
            query: "payment batch processing retry".to_string(),
            pivots,
            neighbors,
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let readable = format_context(&data);
        let compact = format_context_with_mode(&data, OutputFormat::Compact);

        let estimator = TokenEstimator::new();
        let readable_tokens = estimator.estimate_string_hybrid(&readable) as f64;
        let compact_tokens = estimator.estimate_string_hybrid(&compact) as f64;
        let reduction = 1.0 - (compact_tokens / readable_tokens);

        assert!(
            reduction >= 0.15,
            "compact should reduce estimated tokens by >=15% (readable={}, compact={}, reduction={:.1}%)",
            readable_tokens,
            compact_tokens,
            reduction * 100.0
        );
    }

    #[test]
    fn test_compact_format_renders_test_quality_label() {
        let mut pivot = make_pivot(
            "test_compute_security_risk",
            "src/tests/analysis/security_risk_tests.rs",
            10,
            2.0,
            "fn test_compute_security_risk() { ... }",
        );
        pivot.test_quality_label = Some("thorough".to_string());

        let data = ContextData {
            query: "test_compute_security_risk".to_string(),
            pivots: vec![pivot],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context_with_mode(&data, OutputFormat::Compact);
        assert!(
            output.contains("quality=thorough"),
            "compact format should render test_quality_label as quality=thorough, got:\n{}",
            output
        );
    }

    #[test]
    fn test_readable_format_renders_test_quality_label() {
        let mut pivot = make_pivot(
            "test_compute_security_risk",
            "src/tests/analysis/security_risk_tests.rs",
            10,
            2.0,
            "fn test_compute_security_risk() { ... }",
        );
        pivot.test_quality_label = Some("thorough".to_string());

        let data = ContextData {
            query: "test_compute_security_risk".to_string(),
            pivots: vec![pivot],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
        };

        let output = format_context_with_mode(&data, OutputFormat::Readable);
        assert!(
            output.contains("[thorough quality]"),
            "readable format should render test_quality_label as [thorough quality], got:\n{}",
            output
        );
    }

    #[test]
    fn test_no_results_compact_format_avoids_readable_borders() {
        let data = ContextData {
            query: "nonexistent_symbol".to_string(),
            pivots: vec![],
            neighbors: vec![],
            allocation: make_allocation(PivotMode::SignatureOnly, NeighborMode::NameAndLocation),
        };

        let compact = format_context_with_mode(&data, OutputFormat::Compact);
        assert!(
            !compact.contains("==="),
            "compact no-results should not use === readable borders, got:\n{}",
            compact
        );
        assert!(
            compact.contains("no relevant symbols"),
            "compact no-results should contain guidance message"
        );

        // Readable format SHOULD have borders
        let readable = format_context_with_mode(&data, OutputFormat::Readable);
        assert!(
            readable.contains("==="),
            "readable no-results should use === borders"
        );
    }
}
