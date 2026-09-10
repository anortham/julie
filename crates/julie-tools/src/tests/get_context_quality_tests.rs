//! Query-quality regression tests for get_context.

#[cfg(test)]
mod quality_tests {
    use std::sync::Arc;
    use std::time::Instant;

    use julie_index::snapshot::Snapshot;
    use julie_test_support::SnapshotFixture;
    use tempfile::TempDir;

    use crate::get_context::pipeline::run_pipeline;
    use crate::tests::get_context_tests::snapshot_fixture;

    fn setup_quality_fixture() -> (TempDir, SnapshotFixture, Arc<Snapshot>) {
        let sources: Vec<(&str, String)> = [
            (
                "src/auth/validation.rs",
                "validate bearer token and expiry",
                "fn validate_auth_token(token: &str) -> bool",
            ),
            (
                "src/auth/refresh.rs",
                "refresh expired auth token",
                "fn refresh_auth_token(user_id: &str) -> String",
            ),
            (
                "src/payment/processor.rs",
                "process payment transaction",
                "fn process_payment(order_id: &str) -> Result<(), String>",
            ),
            (
                "src/payment/retry.rs",
                "retry failed payment with backoff",
                "fn retry_payment(order_id: &str) -> bool",
            ),
            (
                "src/workspace/resolver.rs",
                "resolve workspace routing and workspace id",
                "fn resolve_workspace_routing(input: &str) -> String",
            ),
            (
                "src/tools/workspace/commands/refresh.rs",
                "refresh workspace index data",
                "fn refresh_workspace_index(workspace: &str) -> bool",
            ),
            (
                "src/search/index.rs",
                "search symbol index",
                "fn search_symbols(query: &str) -> Vec<String>",
            ),
            (
                "src/search/query.rs",
                "build symbol query with and or fallback",
                "fn build_symbol_query(input: &str) -> String",
            ),
            (
                "src/tools/get_context/scoring.rs",
                "select pivots with code fallback",
                "fn select_pivots_with_code_fallback()",
            ),
            (
                "src/tools/get_context/pipeline.rs",
                "run get context pipeline",
                "fn run_pipeline(query: &str) -> String",
            ),
            (
                "src/tools/get_context/formatting.rs",
                "format get context output compact readable",
                "fn format_context_with_mode() -> String",
            ),
            (
                "src/tools/symbols/mod.rs",
                "get file symbols",
                "fn get_symbols(path: &str) -> Vec<String>",
            ),
        ]
        .into_iter()
        .map(|(file, doc, sig)| (file, format!("/// {doc}\n{sig} {{\n    todo!()\n}}\n")))
        .collect();
        let files: Vec<(&str, &str)> = sources
            .iter()
            .map(|(file, content)| (*file, content.as_str()))
            .collect();
        let (dir, fixture) = snapshot_fixture(&files);
        let snapshot = fixture.snapshot();
        (dir, fixture, snapshot)
    }

    #[test]
    fn test_query_quality_hit_rate_on_fixed_dataset() {
        let (_dir, _fixture, snapshot) = setup_quality_fixture();

        let cases: Vec<(&str, &[&str])> = vec![
            ("where auth token is validated", &["validate_auth_token"]),
            ("refresh auth token", &["refresh_auth_token"]),
            ("payment processing", &["process_payment"]),
            ("payment retry behavior", &["retry_payment"]),
            ("workspace routing", &["resolve_workspace_routing"]),
            ("refresh workspace index", &["refresh_workspace_index"]),
            ("search symbols", &["search_symbols"]),
            ("build symbol query", &["build_symbol_query"]),
            (
                "pivot fallback scoring",
                &["select_pivots_with_code_fallback"],
            ),
            ("run context pipeline", &["run_pipeline"]),
            ("compact context formatting", &["format_context_with_mode"]),
            ("get file symbols", &["get_symbols"]),
            ("token validation logic", &["validate_auth_token"]),
            ("workspace id resolution", &["resolve_workspace_routing"]),
            ("payment backoff retry", &["retry_payment"]),
            ("symbol index search", &["search_symbols"]),
            ("or fallback query build", &["build_symbol_query"]),
            ("context output mode", &["format_context_with_mode"]),
            (
                "context pivot selector",
                &["select_pivots_with_code_fallback"],
            ),
            ("workspace refresh command", &["refresh_workspace_index"]),
        ];

        let mut hits = 0;
        for (query, expected_any) in &cases {
            let output = run_pipeline(
                query,
                None,
                None,
                None,
                Some("compact".to_string()),
                &snapshot,
            )
            .unwrap();

            if expected_any.iter().any(|needle| output.contains(needle)) {
                hits += 1;
            }
        }

        let hit_rate = hits as f64 / cases.len() as f64;
        println!(
            "get_context quality hit rate: {}/{} ({:.1}%)",
            hits,
            cases.len(),
            hit_rate * 100.0
        );
        assert!(
            hit_rate >= 0.85,
            "expected >=85% hit rate on fixed dataset, got {}/{} ({:.1}%)",
            hits,
            cases.len(),
            hit_rate * 100.0
        );
    }

    #[test]
    fn test_query_quality_runtime_smoke() {
        let (_dir, _fixture, snapshot) = setup_quality_fixture();
        let queries = [
            "where auth token is validated",
            "payment processing",
            "workspace routing",
            "context output mode",
            "symbol index search",
        ];

        let start = Instant::now();
        for _ in 0..10 {
            for query in &queries {
                let _ = run_pipeline(
                    query,
                    None,
                    None,
                    None,
                    Some("compact".to_string()),
                    &snapshot,
                )
                .unwrap();
            }
        }
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 3000,
            "quality runtime smoke too slow: {} ms",
            elapsed.as_millis()
        );
    }
}
