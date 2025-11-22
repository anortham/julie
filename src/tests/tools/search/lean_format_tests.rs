//! Tests for lean output format in fast_search
//!
//! The lean format is designed for AI agent consumption:
//! - Minimal tokens (80% less than JSON, 60% less than TOON)
//! - Grep-style output familiar to developers
//! - Zero parsing overhead - just read the text

#[cfg(test)]
mod tests {
    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::tools::search::formatting::format_lean_search_results;
    use crate::tools::shared::OptimizedResponse;

    fn make_test_symbol(file_path: &str, line: u32, code_context: &str) -> Symbol {
        Symbol {
            id: format!("test_{}_{}", file_path, line),
            name: "test_symbol".to_string(),
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
            code_context: Some(code_context.to_string()),
            content_type: None,
        }
    }

    #[test]
    fn test_lean_format_single_result() {
        let symbol = make_test_symbol(
            "src/main.rs",
            42,
            "41: // context before\n42→ fn main() {\n43:     println!(\"hello\");",
        );

        let response = OptimizedResponse {
            tool: "fast_search".to_string(),
            results: vec![symbol],
            confidence: 0.9,
            total_found: 1,
            insights: None,
            next_actions: vec![],
        };

        let output = format_lean_search_results("main", &response);

        // Check header
        assert!(output.starts_with("1 matches for \"main\":"));

        // Check file:line header
        assert!(output.contains("src/main.rs:42"));

        // Check code context is indented
        assert!(output.contains("  41: // context before"));
        assert!(output.contains("  42→ fn main()"));
        assert!(output.contains("  43:     println!"));
    }

    #[test]
    fn test_lean_format_multiple_results() {
        let symbols = vec![
            make_test_symbol("src/foo.rs", 10, "10→ fn foo() {"),
            make_test_symbol("src/bar.rs", 20, "20→ fn bar() {"),
            make_test_symbol("src/baz.rs", 30, "30→ fn baz() {"),
        ];

        let response = OptimizedResponse {
            tool: "fast_search".to_string(),
            results: symbols,
            confidence: 0.8,
            total_found: 3,
            insights: None,
            next_actions: vec![],
        };

        let output = format_lean_search_results("fn", &response);

        // Check header shows count
        assert!(output.starts_with("3 matches for \"fn\":"));

        // Check all files are listed
        assert!(output.contains("src/foo.rs:10"));
        assert!(output.contains("src/bar.rs:20"));
        assert!(output.contains("src/baz.rs:30"));

        // Check code contexts
        assert!(output.contains("  10→ fn foo()"));
        assert!(output.contains("  20→ fn bar()"));
        assert!(output.contains("  30→ fn baz()"));
    }

    #[test]
    fn test_lean_format_truncated_results() {
        let symbols = vec![
            make_test_symbol("src/a.rs", 1, "1→ fn a() {"),
            make_test_symbol("src/b.rs", 2, "2→ fn b() {"),
        ];

        let response = OptimizedResponse {
            tool: "fast_search".to_string(),
            results: symbols,
            confidence: 0.7,
            total_found: 100, // More results exist but not shown
            insights: None,
            next_actions: vec![],
        };

        let output = format_lean_search_results("fn", &response);

        // Should show "showing X of Y" when truncated
        assert!(output.contains("2 matches for \"fn\" (showing 2 of 100):"));
    }

    #[test]
    fn test_lean_format_no_code_context() {
        let mut symbol = make_test_symbol("src/test.rs", 5, "");
        symbol.code_context = None;

        let response = OptimizedResponse {
            tool: "fast_search".to_string(),
            results: vec![symbol],
            confidence: 0.5,
            total_found: 1,
            insights: None,
            next_actions: vec![],
        };

        let output = format_lean_search_results("test", &response);

        // Should still have file:line header even without context
        assert!(output.contains("src/test.rs:5"));
    }

    #[test]
    fn test_lean_format_token_efficiency() {
        // Create a realistic search result
        let symbols = vec![
            make_test_symbol(
                "src/tools/search/mod.rs",
                152,
                "151: /// Search implementation\n152→ pub async fn call_tool(&self) -> Result<CallToolResult> {\n153:     let query = &self.query;",
            ),
            make_test_symbol(
                "src/tools/symbols/mod.rs",
                91,
                "90: #[mcp_tool]\n91→ pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {\n92:     debug!(\"get_symbols called\");",
            ),
        ];

        let response = OptimizedResponse {
            tool: "fast_search".to_string(),
            results: symbols,
            confidence: 0.85,
            total_found: 2,
            insights: Some("Mostly Methods".to_string()),
            next_actions: vec!["Use fast_goto".to_string()],
        };

        let lean_output = format_lean_search_results("call_tool", &response);

        // Lean format should be significantly shorter than JSON
        let json_output = serde_json::to_string_pretty(&response).unwrap();

        println!("Lean output ({} chars):\n{}", lean_output.len(), lean_output);
        println!("\nJSON output ({} chars):\n{}", json_output.len(), json_output);

        // Lean should be at least 50% shorter than JSON
        assert!(
            lean_output.len() < json_output.len() / 2,
            "Lean ({}) should be less than half of JSON ({})",
            lean_output.len(),
            json_output.len()
        );
    }

    #[test]
    fn test_lean_format_special_characters() {
        // Test that special characters in code are preserved
        let symbol = make_test_symbol(
            "src/parser.rs",
            100,
            "99: let pattern = r\"\\d+\";\n100→ if value.contains('<') && value.contains('>') {\n101:     // Handle angle brackets",
        );

        let response = OptimizedResponse {
            tool: "fast_search".to_string(),
            results: vec![symbol],
            confidence: 0.9,
            total_found: 1,
            insights: None,
            next_actions: vec![],
        };

        let output = format_lean_search_results("contains", &response);

        // Special chars should be preserved as-is
        assert!(output.contains("r\"\\d+\""));
        assert!(output.contains("'<'"));
        assert!(output.contains("'>'"));
    }
}
