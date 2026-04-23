//! Query-quality regression tests for get_context.

#[cfg(test)]
mod quality_tests {
    use std::time::Instant;

    use tempfile::TempDir;

    use crate::database::{FileInfo, SymbolDatabase};
    use crate::extractors::base::{Symbol, SymbolKind, Visibility};
    use crate::search::index::{SearchIndex, SymbolDocument};
    use crate::tools::get_context::pipeline::run_pipeline;

    fn setup_quality_fixture() -> (TempDir, TempDir, SymbolDatabase, SearchIndex) {
        let db_dir = TempDir::new().unwrap();
        let index_dir = TempDir::new().unwrap();

        let db_path = db_dir.path().join("test.db");
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        let index = SearchIndex::create(index_dir.path()).unwrap();

        let files = vec![
            "src/auth/validation.rs",
            "src/auth/refresh.rs",
            "src/payment/processor.rs",
            "src/payment/retry.rs",
            "src/workspace/resolver.rs",
            "src/tools/workspace/commands/refresh.rs",
            "src/search/index.rs",
            "src/search/query.rs",
            "src/tools/get_context/scoring.rs",
            "src/tools/get_context/pipeline.rs",
            "src/tools/get_context/formatting.rs",
            "src/tools/symbols/mod.rs",
        ];

        for file in files {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 1000,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 2,
                line_count: 0,
                content: None,
            })
            .unwrap();
        }

        let mk = |id: &str, name: &str, file: &str, doc: &str, sig: &str| Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file.to_string(),
            start_line: 1,
            end_line: 20,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            parent_id: None,
            signature: Some(sig.to_string()),
            doc_comment: Some(doc.to_string()),
            visibility: Some(Visibility::Public),
            metadata: None,
            semantic_group: None,
            confidence: Some(0.95),
            code_context: Some(format!("{} {{ /* {} */ }}", sig, doc)),
            content_type: None,
            annotations: Vec::new(),
        };

        let symbols = vec![
            mk(
                "auth_validate",
                "validate_auth_token",
                "src/auth/validation.rs",
                "validate bearer token and expiry",
                "fn validate_auth_token(token: &str) -> bool",
            ),
            mk(
                "auth_refresh",
                "refresh_auth_token",
                "src/auth/refresh.rs",
                "refresh expired auth token",
                "fn refresh_auth_token(user_id: &str) -> String",
            ),
            mk(
                "pay_process",
                "process_payment",
                "src/payment/processor.rs",
                "process payment transaction",
                "fn process_payment(order_id: &str) -> Result<(), String>",
            ),
            mk(
                "pay_retry",
                "retry_payment",
                "src/payment/retry.rs",
                "retry failed payment with backoff",
                "fn retry_payment(order_id: &str) -> bool",
            ),
            mk(
                "ws_resolve",
                "resolve_workspace_routing",
                "src/workspace/resolver.rs",
                "resolve workspace routing and workspace id",
                "fn resolve_workspace_routing(input: &str) -> String",
            ),
            mk(
                "ws_refresh",
                "refresh_workspace_index",
                "src/tools/workspace/commands/refresh.rs",
                "refresh workspace index data",
                "fn refresh_workspace_index(workspace: &str) -> bool",
            ),
            mk(
                "search_symbols",
                "search_symbols",
                "src/search/index.rs",
                "search symbol index",
                "fn search_symbols(query: &str) -> Vec<String>",
            ),
            mk(
                "search_query",
                "build_symbol_query",
                "src/search/query.rs",
                "build symbol query with and or fallback",
                "fn build_symbol_query(input: &str) -> String",
            ),
            mk(
                "ctx_scoring",
                "select_pivots_with_code_fallback",
                "src/tools/get_context/scoring.rs",
                "select pivots with code fallback",
                "fn select_pivots_with_code_fallback()",
            ),
            mk(
                "ctx_pipeline",
                "run_pipeline",
                "src/tools/get_context/pipeline.rs",
                "run get context pipeline",
                "fn run_pipeline(query: &str) -> String",
            ),
            mk(
                "ctx_format",
                "format_context_with_mode",
                "src/tools/get_context/formatting.rs",
                "format get context output compact readable",
                "fn format_context_with_mode() -> String",
            ),
            mk(
                "sym_get",
                "get_symbols",
                "src/tools/symbols/mod.rs",
                "get file symbols",
                "fn get_symbols(path: &str) -> Vec<String>",
            ),
        ];

        db.store_symbols(&symbols).unwrap();
        for sym in &symbols {
            index.add_symbol(&SymbolDocument::from_symbol(sym)).unwrap();
        }
        index.commit().unwrap();

        (db_dir, index_dir, db, index)
    }

    #[test]
    fn test_query_quality_hit_rate_on_fixed_dataset() {
        let (_db_dir, _idx_dir, db, index) = setup_quality_fixture();

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
                &db,
                &index,
                None,
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
        let (_db_dir, _idx_dir, db, index) = setup_quality_fixture();
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
                    &db,
                    &index,
                    None,
                )
                .unwrap();
            }
        }
        let elapsed = start.elapsed();

        // 50 local fixture queries should complete quickly; generous threshold to avoid flakes.
        assert!(
            elapsed.as_millis() < 3000,
            "quality runtime smoke too slow: {} ms",
            elapsed.as_millis()
        );
    }
}
