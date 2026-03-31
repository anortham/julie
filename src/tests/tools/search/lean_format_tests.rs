//! Tests for lean output format in fast_search
//!
//! The lean format is designed for AI agent consumption:
//! - Minimal tokens — no structural overhead
//! - Grep-style output familiar to developers
//! - Zero parsing overhead — just read the text

#[cfg(test)]
mod tests {
    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::tools::search::formatting::{format_lean_search_results, format_locations_only};
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
            results: vec![symbol],
            total_found: 1,
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
            results: symbols,
            total_found: 3,
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
            results: symbols,
            total_found: 100, // More results exist but not shown
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
            results: vec![symbol],
            total_found: 1,
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
            results: symbols,
            total_found: 2,
        };

        let lean_output = format_lean_search_results("call_tool", &response);

        // Lean format should be significantly shorter than JSON
        let json_output = serde_json::to_string_pretty(&response).unwrap();

        println!(
            "Lean output ({} chars):\n{}",
            lean_output.len(),
            lean_output
        );
        println!(
            "\nJSON output ({} chars):\n{}",
            json_output.len(),
            json_output
        );

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
            results: vec![symbol],
            total_found: 1,
        };

        let output = format_lean_search_results("contains", &response);

        // Special chars should be preserved as-is
        assert!(output.contains("r\"\\d+\""));
        assert!(output.contains("'<'"));
        assert!(output.contains("'>'"));
    }

    #[test]
    fn test_locations_only_format() {
        let mut sym1 = make_test_symbol(
            "src/tools/search/mod.rs",
            42,
            "41: // context before\n42→ pub fn call_tool() {\n43:     // body",
        );
        sym1.name = "call_tool".to_string();
        sym1.kind = SymbolKind::Function;

        let mut sym2 = make_test_symbol("src/lib.rs", 10, "10→ pub struct Foo {");
        sym2.name = "Foo".to_string();
        sym2.kind = SymbolKind::Struct;

        let response = OptimizedResponse {
            results: vec![sym1, sym2],
            total_found: 2,
        };

        let output = format_locations_only("call_tool", &response);

        // Header should mention count and query
        assert!(output.contains("2 locations for \"call_tool\""));

        // Should contain file:line for each result
        assert!(output.contains("src/tools/search/mod.rs:42"));
        assert!(output.contains("src/lib.rs:10"));

        // Should contain kind in parens
        assert!(output.contains("(function)"));
        assert!(output.contains("(struct)"));

        // Must NOT contain code context lines
        assert!(!output.contains("41: // context before"));
        assert!(!output.contains("42→ pub fn call_tool()"));
        assert!(!output.contains("10→ pub struct Foo"));

        // Output should be compact: one header line + one line per result
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3, "Expected 1 header + 2 result lines, got: {output}");
    }

    #[test]
    fn test_locations_only_format_truncated() {
        let symbols: Vec<Symbol> = (0..3)
            .map(|i| make_test_symbol(&format!("src/file{i}.rs"), i * 10, "code"))
            .collect();

        let response = OptimizedResponse {
            results: symbols,
            total_found: 50,
        };

        let output = format_locations_only("foo", &response);

        // When count < total, show "showing N of M"
        assert!(output.contains("3 locations for \"foo\" (showing 3 of 50)"));
    }

    #[test]
    fn test_lean_format_groups_same_file_results() {
        // 2 matches in src/handler.rs, 1 in src/other.rs
        let symbols = vec![
            make_test_symbol("src/handler.rs", 42, "42→ fn foo() {"),
            make_test_symbol("src/handler.rs", 100, "100→ fn bar() {"),
            make_test_symbol("src/other.rs", 5, "5→ fn baz() {"),
        ];

        let response = OptimizedResponse {
            results: symbols,
            total_found: 3,
        };

        let output = format_lean_search_results("fn", &response);

        // File path for the grouped file should appear only ONCE
        let handler_occurrences = output.matches("src/handler.rs").count();
        assert_eq!(
            handler_occurrences, 1,
            "src/handler.rs should appear exactly once (grouped header), got {handler_occurrences}. Output:\n{output}"
        );

        // The grouped header should be file: (no line number)
        assert!(
            output.contains("src/handler.rs:\n"),
            "Grouped file should use 'file:' header (no line number). Output:\n{output}"
        );

        // Both code contexts should be indented under the group
        assert!(output.contains("  42→ fn foo()"), "First match context missing. Output:\n{output}");
        assert!(output.contains("  100→ fn bar()"), "Second match context missing. Output:\n{output}");

        // Single-match file can use either format; just verify it appears
        assert!(output.contains("src/other.rs"), "Other file should appear. Output:\n{output}");
        assert!(output.contains("  5→ fn baz()"), "Third match context missing. Output:\n{output}");
    }

    #[test]
    fn test_lean_format_grouped_results_include_line_anchors() {
        // Two matches in the same file — the grouped output must emit `:line` anchors
        // so callers can locate each match without code context.
        let symbols = vec![
            make_test_symbol("src/handler.rs", 42, "42→ fn foo() {"),
            make_test_symbol("src/handler.rs", 100, "100→ fn bar() {"),
        ];

        let response = OptimizedResponse {
            results: symbols,
            total_found: 2,
        };

        let output = format_lean_search_results("fn", &response);

        // File header must use grouped form (no line number in the header itself).
        assert!(
            output.contains("src/handler.rs:\n"),
            "grouped header should have no line number. Output:\n{output}"
        );

        // Each match must have a :line anchor under the file header.
        assert!(
            output.contains("  :42\n"),
            "first match must have :42 line anchor. Output:\n{output}"
        );
        assert!(
            output.contains("  :100\n"),
            "second match must have :100 line anchor. Output:\n{output}"
        );

        // Code context should be indented under its anchor.
        assert!(
            output.contains("    42→ fn foo()"),
            "code context should be indented under anchor. Output:\n{output}"
        );
        assert!(
            output.contains("    100→ fn bar()"),
            "code context should be indented under anchor. Output:\n{output}"
        );
    }

    #[test]
    fn test_definition_search_grouped_other_matches_include_line_anchors() {
        use crate::extractors::base::{Symbol, SymbolKind};
        use crate::tools::search::formatting::format_definition_search_results;
        use crate::tools::shared::OptimizedResponse;

        // Exact match + two "other" matches in the same file — verify `:line` anchors in
        // the "Other matches" grouped section.
        let mut exact = make_test_symbol("src/router.rs", 5, "5→ struct Router {");
        exact.name = "Router".to_string();
        exact.kind = SymbolKind::Struct;

        let mut other1 = make_test_symbol("src/middleware.rs", 20, "20→ fn use_router() {");
        other1.name = "use_router".to_string();
        other1.kind = SymbolKind::Function;

        let mut other2 = make_test_symbol("src/middleware.rs", 80, "80→ fn mount_router() {");
        other2.name = "mount_router".to_string();
        other2.kind = SymbolKind::Function;

        let response = OptimizedResponse {
            results: vec![exact, other1, other2],
            total_found: 3,
        };

        let output = format_definition_search_results("Router", &response);

        // "Other matches" grouped section must include line anchors.
        assert!(
            output.contains("  :20\n"),
            "first other match must have :20 line anchor in grouped section. Output:\n{output}"
        );
        assert!(
            output.contains("  :80\n"),
            "second other match must have :80 line anchor in grouped section. Output:\n{output}"
        );
    }

    #[test]
    fn test_fast_search_return_format_deserialization() {
        use crate::tools::search::FastSearchTool;

        // Default should be "full"
        let tool: FastSearchTool = serde_json::from_str(r#"{"query": "test"}"#).unwrap();
        assert_eq!(tool.return_format, "full");

        // Explicit "locations"
        let tool: FastSearchTool =
            serde_json::from_str(r#"{"query": "test", "return_format": "locations"}"#).unwrap();
        assert_eq!(tool.return_format, "locations");
    }
}
