// Search Tools Tests - Token Optimization and Response Formatting
// Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod search_tools_tests {
    use crate::extractors::base::Visibility;
    use crate::extractors::{Symbol, SymbolKind};
    use crate::tools::search::formatting::format_optimized_results;
    use crate::tools::search::FastSearchTool;
    use crate::tools::shared::OptimizedResponse;

    #[test]
    fn test_short_response_unchanged() {
        let search_tool = FastSearchTool {
            query: "test".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            scope: "content".to_string(),
            output: None,
            context_lines: None,
        };

        let symbols = vec![Symbol {
            id: "1".to_string(),
            name: "testFunction".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 3,
            end_column: 1,
            start_byte: 0,
            end_byte: 50,
            signature: Some("fn testFunction() -> bool".to_string()),
            doc_comment: None,
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata: Some(std::collections::HashMap::new()),
            semantic_group: None,
            confidence: Some(0.95),
            code_context: Some("fn testFunction() -> bool {\n    true\n}".to_string()),
        }];

        let optimized = OptimizedResponse {
            tool: "test".to_string(),
            results: symbols,
            confidence: 0.95,
            total_found: 1,
            insights: Some("Found test function".to_string()),
            next_actions: vec!["Examine implementation".to_string()],
        };

        let result = format_optimized_results(&search_tool.query, &optimized);

        // NEW FORMAT: Minimal 2-line output for AI agents
        // Line 1: Summary with count and confidence
        assert!(result.contains("Found 1 results for 'test'"));
        assert!(result.contains("confidence: 0.9")); // Rounded to 1 decimal

        // Line 2: Top result names
        assert!(result.contains("Top results:"));
        assert!(result.contains("testFunction"));

        // Should NOT contain code context (agents get that in structured_content)
        assert!(!result.contains("fn testFunction() -> bool {"));
        assert!(!result.contains("true\n}"));
    }

    #[test]
    fn test_context_truncation_with_long_code() {
        let search_tool = FastSearchTool {
            query: "longFunction".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            scope: "content".to_string(),
            output: None,
            context_lines: None,
        };

        // Create a symbol with more than 10 lines of context
        let long_context = (1..=15)
            .map(|i| format!("    line_{}", i))
            .collect::<Vec<_>>()
            .join("\n");

        let symbols = vec![Symbol {
            id: "1".to_string(),
            name: "longFunction".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 15,
            end_column: 1,
            start_byte: 0,
            end_byte: 200,
            signature: Some("fn longFunction()".to_string()),
            doc_comment: None,
            visibility: Some(Visibility::Public),
            parent_id: None,
            metadata: Some(std::collections::HashMap::new()),
            semantic_group: None,
            confidence: Some(0.95),
            code_context: Some(long_context),
        }];

        let optimized = OptimizedResponse {
            tool: "test".to_string(),
            results: symbols,
            confidence: 0.95,
            total_found: 1,
            insights: None,
            next_actions: vec![],
        };

        let result = format_optimized_results(&search_tool.query, &optimized);

        // NEW FORMAT: Minimal output regardless of context length
        assert!(result.contains("Found 1 results for 'longFunction'"));
        assert!(result.contains("Top results:"));
        assert!(result.contains("longFunction"));

        // Should NOT contain truncation messages (no context in text output)
        assert!(!result.contains("more lines truncated"));
        assert!(!result.contains("line_1"));
        assert!(!result.contains("line_15"));
    }

    #[test]
    fn test_token_limit_early_termination() {
        // NEW: With minimal output, token limits are no longer relevant
        // Text output is always 2 lines regardless of result count

        let search_tool = FastSearchTool {
            query: "manyResults".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 100,
            workspace: Some("primary".to_string()),
            scope: "content".to_string(),
            output: None,
            context_lines: None,
        };

        // Create many symbols - text output will remain minimal
        let large_context = (1..=10)
            .map(|i| format!("    // Comment line {} with content", i))
            .collect::<Vec<_>>()
            .join("\n");

        let mut symbols = Vec::new();
        for i in 1..=70 {
            symbols.push(Symbol {
                id: i.to_string(),
                name: format!("function_{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: format!("file_{}.rs", i),
                start_line: 1,
                start_column: 0,
                end_line: 10,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                signature: Some(format!("fn function_{}() -> Result<(), Error>", i)),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(std::collections::HashMap::new()),
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(large_context.clone()),
            });
        }

        let optimized = OptimizedResponse {
            tool: "test".to_string(),
            results: symbols,
            confidence: 0.9,
            total_found: 70,
            insights: None,
            next_actions: vec![],
        };

        let result = format_optimized_results(&search_tool.query, &optimized);

        // NEW FORMAT: Minimal output shows top 5 names only
        assert!(result.contains("Found 70 results for 'manyResults'"));
        assert!(result.contains("Top results:"));

        // Shows only top 5 names
        assert!(result.contains("function_1"));
        assert!(result.contains("function_5"));

        // Does NOT show all 70 (only top 5)
        assert!(!result.contains("function_70"));

        // NO token limit messages (not needed with minimal output)
        assert!(!result.contains("Applied progressive reduction"));
        assert!(!result.contains("Response truncated"));
    }

    #[test]
    fn test_progressive_reduction_integration() {
        // NEW: Progressive reduction no longer exists - minimal output is always 2 lines

        let search_tool = FastSearchTool {
            query: "manyResults".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 100,
            workspace: Some("primary".to_string()),
            scope: "content".to_string(),
            output: None,
            context_lines: None,
        };

        // Create 80 symbols - text output will remain minimal
        let large_context = (1..=20)
            .map(|i| format!("    // Large context line {} with content", i))
            .collect::<Vec<_>>()
            .join("\n");

        let mut symbols = Vec::new();
        for i in 1..=80 {
            symbols.push(Symbol {
                id: i.to_string(),
                name: format!("function_{}", i),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: format!("file_{}.rs", i),
                start_line: 1,
                start_column: 0,
                end_line: 20,
                end_column: 0,
                start_byte: 0,
                end_byte: 100,
                signature: Some(format!("fn function_{}() -> Result<(), Error>", i)),
                doc_comment: None,
                visibility: Some(Visibility::Public),
                parent_id: None,
                metadata: Some(std::collections::HashMap::new()),
                semantic_group: None,
                confidence: Some(0.9),
                code_context: Some(large_context.clone()),
            });
        }

        let optimized = OptimizedResponse {
            tool: "test".to_string(),
            results: symbols,
            confidence: 0.9,
            total_found: 80,
            insights: None,
            next_actions: vec![],
        };

        let result = format_optimized_results(&search_tool.query, &optimized);

        // NEW FORMAT: Minimal 2-line output
        assert!(result.contains("Found 80 results for 'manyResults'"));
        assert!(result.contains("Top results:"));

        // Shows top 5 names only
        assert!(result.contains("function_1"));

        // Does NOT show all 80 results
        assert!(!result.contains("function_80"));

        // NO progressive reduction messages
        assert!(!result.contains("Applied progressive reduction"));
        assert!(!result.contains("→"));
    }

    #[test]
    fn test_multi_word_query_routing_bug() {
        // This test reproduces the bug where multi-word queries fail with primary workspace
        // but work with "all" workspace due to routing to database vs Tantivy

        // Test case: Multi-word query like "fast search" should work regardless of workspace filter
        // The bug is that workspace="primary" routes to database LIKE search which can't handle spaces
        // while workspace="all" routes to Tantivy which can handle multi-word queries

        let multi_word_query = "fast search";

        // Test 1: workspace="primary" (default) - this currently fails due to database routing
        let search_tool_primary = FastSearchTool {
            query: multi_word_query.to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            scope: "content".to_string(),
            output: None,
            context_lines: None,
        };

        // Test 2: workspace="all" - this works because it routes to Tantivy
        let search_tool_all = FastSearchTool {
            query: multi_word_query.to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("all".to_string()),
            scope: "content".to_string(),
            output: None,
            context_lines: None,
        };

        // Note: This test documents the bug but doesn't actually test the routing logic
        // because that requires a full handler setup with database and Tantivy.
        // The real test is demonstrated through manual testing:
        // - workspace="primary": No results for "fast search"
        // - workspace="all": Found results for "fast search"

        // For now, we verify the tool configurations are correct
        assert_eq!(search_tool_primary.query, "fast search");
        assert_eq!(search_tool_primary.workspace.as_ref().unwrap(), "primary");

        assert_eq!(search_tool_all.query, "fast search");
        assert_eq!(search_tool_all.workspace.as_ref().unwrap(), "all");

        // TODO: Add integration test with actual handler setup to verify:
        // 1. search_tool_primary routes to database_search_with_workspace_filter
        // 2. search_tool_all routes to Tantivy search engine
        // 3. Multi-word queries work with both routing paths after fix
    }
}
