// Search Tools Tests - Token Optimization and Response Formatting
// Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod search_tools_tests {
    use crate::extractors::base::Visibility;
    use crate::extractors::{Symbol, SymbolKind};
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

        let result = search_tool.format_optimized_results(&optimized);

        // Should contain the function name and context
        assert!(result.contains("testFunction"));
        assert!(result.contains("fn testFunction() -> bool {"));
        assert!(result.contains("true"));

        // Should not contain truncation messages for short content
        assert!(!result.contains("truncated"));
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

        let result = search_tool.format_optimized_results(&optimized);

        // Should contain the function name
        assert!(result.contains("longFunction"));

        // Should show truncation message since context > 10 lines
        assert!(result.contains("more lines truncated"));
        assert!(result.contains("(5 more lines truncated"));

        // Should contain some context lines but not all
        assert!(result.contains("line_1"));
        assert!(!result.contains("line_15")); // Last line should be truncated
    }

    #[test]
    fn test_token_limit_early_termination() {
        // This test verifies that we stop adding results when approaching token limits
        // We'll need to create enough symbols with large contexts to trigger the limit

        let search_tool = FastSearchTool {
            query: "manyResults".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 100,
            workspace: Some("primary".to_string()),
        };

        // Create many symbols with substantial context to trigger token limit
        let large_context = (1..=10).map(|i| format!("    // This is a comment line {} with substantial content that will contribute to token count", i)).collect::<Vec<_>>().join("\n");

        let mut symbols = Vec::new();
        for i in 1..=70 {
            // Create 70 symbols to definitely exceed 15K token limits (was 50)
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

        let result = search_tool.format_optimized_results(&optimized);

        // Should contain either progressive reduction notice OR early termination
        let has_progressive_reduction = result.contains("Applied progressive reduction");
        let has_early_termination =
            result.contains("Response truncated to stay within token limits");
        assert!(has_progressive_reduction || has_early_termination);

        // Should not contain all 70 functions due to token limiting
        assert!(result.contains("function_1"));
        assert!(!result.contains("function_70")); // Last functions should be excluded
    }

    #[test]
    fn test_progressive_reduction_integration() {
        let search_tool = FastSearchTool {
            query: "manyResults".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 100,
            workspace: Some("primary".to_string()),
        };

        // Create enough symbols to trigger progressive reduction (not just early termination)
        let large_context = (1..=20).map(|i| format!("    // Large context line {} with substantial content for progressive reduction", i)).collect::<Vec<_>>().join("\n");

        let mut symbols = Vec::new();
        for i in 1..=80 {
            // Create 80 symbols to definitely exceed rough estimate
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

        let result = search_tool.format_optimized_results(&optimized);

        // Should contain progressive reduction notice
        assert!(result.contains("Applied progressive reduction"));
        assert!(result.contains("→")); // The reduction arrow

        // Should contain first function but not all 80
        assert!(result.contains("function_1"));
        assert!(!result.contains("function_80"));

        // Should show accurate count (not 80)
        assert!(!result.contains("Showing 80 of 80"));
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
        };

        // Test 2: workspace="all" - this works because it routes to Tantivy
        let search_tool_all = FastSearchTool {
            query: multi_word_query.to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("all".to_string()),
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
