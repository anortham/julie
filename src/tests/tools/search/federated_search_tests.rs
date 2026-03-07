//! Tests for federated fast_search: formatting with project tags and routing.
//!
//! Tests cover:
//! - `format_federated_lean_results`: content search with [project: name] tags
//! - `format_federated_definition_results`: definition search with project tags + promotion
//! - `count_unique` helper
//! - Error message when attempting federated search in stdio mode

#[cfg(test)]
mod tests {
    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::tools::search::formatting::{
        format_federated_definition_results, format_federated_lean_results,
    };
    use crate::tools::shared::OptimizedResponse;

    fn make_symbol(name: &str, file_path: &str, line: u32, code_context: Option<&str>) -> Symbol {
        Symbol {
            id: format!("test_{}_{}", file_path, line),
            name: name.to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 5,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            parent_id: None,
            signature: None,
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: code_context.map(|s| s.to_string()),
            content_type: None,
        }
    }

    // =======================================================================
    // format_federated_lean_results
    // =======================================================================

    #[test]
    fn test_federated_lean_single_project() {
        let symbols = vec![
            make_symbol("foo", "src/foo.rs", 10, Some("10: fn foo() {")),
        ];
        let response = OptimizedResponse {
            results: symbols,
            total_found: 1,
        };
        let project_names = vec!["my-project".to_string()];

        let output = format_federated_lean_results("foo", &response, &project_names);

        assert!(
            output.contains("across 1 projects"),
            "Should show project count. Got:\n{}",
            output
        );
        assert!(
            output.contains("[project: my-project] src/foo.rs:10"),
            "Should tag result with project name. Got:\n{}",
            output
        );
        assert!(
            output.contains("  10: fn foo()"),
            "Should include indented code context. Got:\n{}",
            output
        );
    }

    #[test]
    fn test_federated_lean_multiple_projects() {
        let symbols = vec![
            make_symbol("search", "src/search.rs", 42, Some("42: fn search() {")),
            make_symbol("search", "src/find.rs", 100, Some("100: fn search() {")),
            make_symbol("query", "src/query.rs", 5, Some("5: fn query() {")),
        ];
        let response = OptimizedResponse {
            results: symbols,
            total_found: 3,
        };
        let project_names = vec![
            "alpha".to_string(),
            "beta".to_string(),
            "alpha".to_string(),
        ];

        let output = format_federated_lean_results("search", &response, &project_names);

        assert!(
            output.contains("across 2 projects"),
            "Should count unique projects. Got:\n{}",
            output
        );
        assert!(
            output.contains("[project: alpha] src/search.rs:42"),
            "First result tagged with alpha"
        );
        assert!(
            output.contains("[project: beta] src/find.rs:100"),
            "Second result tagged with beta"
        );
        assert!(
            output.contains("[project: alpha] src/query.rs:5"),
            "Third result tagged with alpha"
        );
    }

    #[test]
    fn test_federated_lean_truncated() {
        let symbols = vec![
            make_symbol("a", "src/a.rs", 1, None),
            make_symbol("b", "src/b.rs", 2, None),
        ];
        let response = OptimizedResponse {
            results: symbols,
            total_found: 50,
        };
        let project_names = vec!["proj-a".to_string(), "proj-b".to_string()];

        let output = format_federated_lean_results("test", &response, &project_names);

        assert!(
            output.contains("showing 2 of 50"),
            "Should show truncation info. Got:\n{}",
            output
        );
    }

    #[test]
    fn test_federated_lean_no_code_context() {
        let symbols = vec![make_symbol("foo", "src/foo.rs", 10, None)];
        let response = OptimizedResponse {
            results: symbols,
            total_found: 1,
        };
        let project_names = vec!["my-project".to_string()];

        let output = format_federated_lean_results("foo", &response, &project_names);

        // Should still have the project-tagged file:line header
        assert!(
            output.contains("[project: my-project] src/foo.rs:10"),
            "Should have project tag even without code context"
        );
    }

    // =======================================================================
    // format_federated_definition_results
    // =======================================================================

    #[test]
    fn test_federated_definition_exact_match_promoted() {
        let symbols = vec![
            make_symbol("SearchIndex", "src/search.rs", 42, None),
            make_symbol("search_impl", "src/search.rs", 100, Some("100: fn search_impl() {")),
        ];
        let response = OptimizedResponse {
            results: symbols,
            total_found: 2,
        };
        let project_names = vec!["julie".to_string(), "julie".to_string()];

        let output =
            format_federated_definition_results("SearchIndex", &response, &project_names);

        assert!(
            output.contains("Definition found: SearchIndex"),
            "Should have promoted header. Got:\n{}",
            output
        );
        assert!(
            output.contains("[project: julie] src/search.rs:42"),
            "Promoted section should have project tag. Got:\n{}",
            output
        );
        assert!(
            output.contains("Other matches:"),
            "Should have other matches section. Got:\n{}",
            output
        );
        assert!(
            output.contains("[project: julie] src/search.rs:100"),
            "Other matches should have project tag. Got:\n{}",
            output
        );
    }

    #[test]
    fn test_federated_definition_exact_match_across_projects() {
        // Same symbol name found in different projects
        let mut sym1 = make_symbol("Config", "src/config.rs", 10, None);
        sym1.kind = SymbolKind::Class;
        let mut sym2 = make_symbol("Config", "lib/config.py", 5, None);
        sym2.kind = SymbolKind::Class;
        let sym3 = make_symbol("load_config", "src/loader.rs", 50, None);

        let response = OptimizedResponse {
            results: vec![sym1, sym2, sym3],
            total_found: 3,
        };
        let project_names = vec![
            "backend".to_string(),
            "frontend".to_string(),
            "backend".to_string(),
        ];

        let output =
            format_federated_definition_results("Config", &response, &project_names);

        assert!(
            output.contains("Definition found: Config"),
            "Should promote Config"
        );
        // Both Config entries should be in promoted section
        assert!(
            output.contains("[project: backend] src/config.rs:10"),
            "Backend Config should be promoted"
        );
        assert!(
            output.contains("[project: frontend] lib/config.py:5"),
            "Frontend Config should be promoted"
        );
        // load_config should be in Other matches
        assert!(
            output.contains("[project: backend] src/loader.rs:50"),
            "load_config should be in other matches"
        );
    }

    #[test]
    fn test_federated_definition_no_exact_match_fallback() {
        // When no exact match exists, should fall back to lean format
        let symbols = vec![
            make_symbol("search_impl", "src/search.rs", 42, Some("42: fn search_impl() {")),
            make_symbol("search_all", "src/search.rs", 100, Some("100: fn search_all() {")),
        ];
        let response = OptimizedResponse {
            results: symbols,
            total_found: 2,
        };
        let project_names = vec!["alpha".to_string(), "beta".to_string()];

        let output =
            format_federated_definition_results("search", &response, &project_names);

        // Should NOT have "Definition found:" since no exact match
        assert!(
            !output.contains("Definition found:"),
            "No exact match means no promoted section. Got:\n{}",
            output
        );
        // Should use the lean federated format
        assert!(
            output.contains("[project: alpha] src/search.rs:42"),
            "Should use federated lean format. Got:\n{}",
            output
        );
        assert!(
            output.contains("[project: beta] src/search.rs:100"),
            "Should tag second result. Got:\n{}",
            output
        );
    }

    #[test]
    fn test_federated_definition_with_signature() {
        let mut sym = make_symbol("parse", "src/parser.rs", 10, None);
        sym.signature = Some("pub fn parse(input: &str) -> Result<AST>".to_string());

        let response = OptimizedResponse {
            results: vec![sym],
            total_found: 1,
        };
        let project_names = vec!["parser-lib".to_string()];

        let output =
            format_federated_definition_results("parse", &response, &project_names);

        assert!(
            output.contains("Definition found: parse"),
            "Should promote exact match"
        );
        assert!(
            output.contains("pub fn parse(input: &str) -> Result<AST>"),
            "Should show signature in promoted section. Got:\n{}",
            output
        );
    }

    // =======================================================================
    // Conversion function visibility tests (pub(crate))
    // =======================================================================

    #[test]
    fn test_tantivy_symbol_to_symbol_accessible() {
        use crate::search::index::SymbolSearchResult;
        use crate::tools::search::text_search::tantivy_symbol_to_symbol;

        let result = SymbolSearchResult {
            id: "sym_1".to_string(),
            name: "my_function".to_string(),
            kind: "function".to_string(),
            language: "rust".to_string(),
            file_path: "src/lib.rs".to_string(),
            start_line: 42,
            signature: "pub fn my_function()".to_string(),
            doc_comment: "Does stuff".to_string(),
            score: 0.95,
        };

        let symbol = tantivy_symbol_to_symbol(result);

        assert_eq!(symbol.name, "my_function");
        assert_eq!(symbol.file_path, "src/lib.rs");
        assert_eq!(symbol.start_line, 42);
        assert_eq!(symbol.kind, SymbolKind::Function);
        assert_eq!(symbol.signature, Some("pub fn my_function()".to_string()));
        assert_eq!(symbol.doc_comment, Some("Does stuff".to_string()));
        assert_eq!(symbol.confidence, Some(0.95));
    }

    #[test]
    fn test_content_result_to_symbol_accessible() {
        use crate::search::index::ContentSearchResult;
        use crate::tools::search::text_search::content_result_to_symbol;

        let result = ContentSearchResult {
            file_path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            score: 0.8,
        };

        let symbol = content_result_to_symbol(result);

        assert_eq!(symbol.file_path, "src/main.rs");
        assert_eq!(symbol.language, "rust");
        assert_eq!(symbol.kind, SymbolKind::Module);
        assert_eq!(symbol.confidence, Some(0.8));
        assert_eq!(
            symbol.semantic_group,
            Some("content_match".to_string())
        );
    }

    // =======================================================================
    // count_unique (tested indirectly via formatting output)
    // =======================================================================

    #[test]
    fn test_federated_lean_project_count_deduplication() {
        // 4 results across 2 unique projects
        let symbols = vec![
            make_symbol("a", "src/a.rs", 1, None),
            make_symbol("b", "src/b.rs", 2, None),
            make_symbol("c", "src/c.rs", 3, None),
            make_symbol("d", "src/d.rs", 4, None),
        ];
        let response = OptimizedResponse {
            results: symbols,
            total_found: 4,
        };
        let project_names = vec![
            "proj-x".to_string(),
            "proj-y".to_string(),
            "proj-x".to_string(),
            "proj-y".to_string(),
        ];

        let output = format_federated_lean_results("test", &response, &project_names);

        assert!(
            output.contains("across 2 projects"),
            "Should count 2 unique projects, not 4. Got:\n{}",
            output
        );
    }
}
