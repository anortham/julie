//! Tests for exact-match promotion in definition search
//!
//! When search_target="definitions" and a result is an exact name match
//! for the query, it should be promoted to the top with richer formatting.

#[cfg(test)]
mod tests {
    use crate::extractors::base::{Symbol, SymbolKind, Visibility};
    use crate::tools::search::formatting::{
        format_definition_search_results, format_lean_search_results,
    };
    use crate::tools::shared::OptimizedResponse;

    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        file_path: &str,
        line: u32,
        signature: Option<&str>,
        visibility: Option<Visibility>,
        code_context: Option<&str>,
    ) -> Symbol {
        Symbol {
            id: format!("test_{}_{}_{}", name, file_path, line),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: line,
            end_line: line + 5,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            parent_id: None,
            signature: signature.map(|s| s.to_string()),
            doc_comment: None,
            visibility,
            metadata: None,
            semantic_group: None,
            confidence: Some(0.9),
            code_context: code_context.map(|s| s.to_string()),
            content_type: None,
        }
    }

    #[test]
    fn test_exact_match_promoted_to_top() {
        let symbols = vec![
            make_symbol(
                "search_index",
                SymbolKind::Function,
                "src/utils.rs",
                10,
                Some("fn search_index()"),
                None,
                Some("fn search_index() { ... }"),
            ),
            make_symbol(
                "SearchIndex",
                SymbolKind::Class,
                "src/search/index.rs",
                108,
                Some("pub struct SearchIndex"),
                Some(Visibility::Public),
                Some("pub struct SearchIndex { ... }"),
            ),
        ];

        let response = OptimizedResponse::new("fast_search", symbols, 0.9);
        let output = format_definition_search_results("SearchIndex", &response);

        // Exact match should come first with "Definition found:" header
        assert!(
            output.starts_with("Definition found: SearchIndex"),
            "Expected 'Definition found:' header at top, got:\n{}",
            output
        );

        // Should show kind and visibility
        assert!(output.contains("struct"), "Should show kind");
        assert!(output.contains("public"), "Should show visibility");

        // Other results should follow
        assert!(output.contains("Other matches:"), "Should have 'Other matches:' section");
        assert!(output.contains("src/utils.rs"), "Other result should be present");
    }

    #[test]
    fn test_no_exact_match_uses_standard_format() {
        let symbols = vec![
            make_symbol(
                "search_index_impl",
                SymbolKind::Function,
                "src/search.rs",
                42,
                Some("fn search_index_impl()"),
                None,
                Some("fn search_index_impl() { ... }"),
            ),
            make_symbol(
                "build_search_index",
                SymbolKind::Function,
                "src/indexing.rs",
                15,
                Some("fn build_search_index()"),
                None,
                Some("fn build_search_index() { ... }"),
            ),
        ];

        let response = OptimizedResponse::new("fast_search", symbols, 0.7);
        let output = format_definition_search_results("SearchIndex", &response);

        // No exact match → standard format (no "Definition found:" header)
        assert!(
            !output.contains("Definition found:"),
            "Should NOT have 'Definition found:' without exact match"
        );
        assert!(
            !output.contains("Other matches:"),
            "Should NOT have 'Other matches:' without exact match"
        );

        // Should still show the results in the standard format
        let standard = format_lean_search_results("SearchIndex", &response);
        assert_eq!(output, standard, "Without exact match, should fall back to standard format");
    }

    #[test]
    fn test_exact_match_case_sensitive() {
        let symbols = vec![
            make_symbol(
                "searchindex",
                SymbolKind::Function,
                "src/utils.rs",
                10,
                None,
                None,
                Some("fn searchindex() {}"),
            ),
            make_symbol(
                "SearchIndex",
                SymbolKind::Class,
                "src/search.rs",
                20,
                Some("pub struct SearchIndex"),
                Some(Visibility::Public),
                Some("pub struct SearchIndex { ... }"),
            ),
        ];

        let response = OptimizedResponse::new("fast_search", symbols, 0.9);
        let output = format_definition_search_results("SearchIndex", &response);

        // Case-sensitive: "SearchIndex" matches but "searchindex" does not
        assert!(output.contains("Definition found: SearchIndex"));
        assert!(output.contains("Other matches:"));
    }

    #[test]
    fn test_multiple_exact_matches() {
        // Same symbol name defined in multiple files
        let symbols = vec![
            make_symbol(
                "Config",
                SymbolKind::Class,
                "src/config/mod.rs",
                10,
                Some("pub struct Config"),
                Some(Visibility::Public),
                Some("pub struct Config { ... }"),
            ),
            make_symbol(
                "Config",
                SymbolKind::Class,
                "src/tests/fixtures.rs",
                25,
                Some("struct Config"),
                Some(Visibility::Private),
                Some("struct Config { ... }"),
            ),
            make_symbol(
                "load_config",
                SymbolKind::Function,
                "src/config/loader.rs",
                5,
                Some("fn load_config()"),
                None,
                Some("fn load_config() { ... }"),
            ),
        ];

        let response = OptimizedResponse::new("fast_search", symbols, 0.9);
        let output = format_definition_search_results("Config", &response);

        // Both exact matches should appear in the promoted section
        assert!(output.contains("Definition found: Config"));
        assert!(output.contains("src/config/mod.rs"));
        assert!(output.contains("src/tests/fixtures.rs"));

        // Non-exact match in "Other matches:"
        assert!(output.contains("Other matches:"));
        assert!(output.contains("src/config/loader.rs"));
    }

    #[test]
    fn test_exact_match_shows_signature() {
        let symbols = vec![make_symbol(
            "process_payment",
            SymbolKind::Function,
            "src/payment.rs",
            42,
            Some("pub fn process_payment(order: &Order) -> Result<Receipt>"),
            Some(Visibility::Public),
            Some("pub fn process_payment(order: &Order) -> Result<Receipt> { ... }"),
        )];

        let response = OptimizedResponse::new("fast_search", symbols, 0.95);
        let output = format_definition_search_results("process_payment", &response);

        assert!(output.contains("Definition found: process_payment"));
        assert!(output.contains("pub fn process_payment(order: &Order) -> Result<Receipt>"));
        assert!(output.contains("src/payment.rs:42"));

        // Only exact matches, no "Other matches:" section
        assert!(!output.contains("Other matches:"));
    }

    #[test]
    fn test_exact_match_without_visibility_omits_it() {
        let symbols = vec![make_symbol(
            "helper_fn",
            SymbolKind::Function,
            "src/utils.rs",
            10,
            Some("fn helper_fn()"),
            None, // No visibility info
            Some("fn helper_fn() {}"),
        )];

        let response = OptimizedResponse::new("fast_search", symbols, 0.9);
        let output = format_definition_search_results("helper_fn", &response);

        assert!(output.contains("Definition found: helper_fn"));
        assert!(output.contains("function")); // kind is shown
        // No visibility string when it's None
        assert!(!output.contains("public"));
        assert!(!output.contains("private"));
    }
}
