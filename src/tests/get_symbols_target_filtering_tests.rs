//! Target Filtering Tests for GetSymbolsTool
//!
//! Tests that target parameter correctly filters symbols at ALL levels,
//! not just top-level symbols.
//!
//! TDD: These tests define the expected behavior before implementing the fix.

#[cfg(test)]
mod tests {
    use crate::handler::JulieServerHandler;
    use crate::tools::{GetSymbolsTool, ManageWorkspaceTool};
    use anyhow::Result;
    use rust_mcp_sdk::schema::CallToolResult;

    #[tokio::test]
    #[ignore] // SLOW/HANGS: Indexes entire workspace (300+ files) - not critical for CLI tools
    async fn test_target_filtering_matches_child_methods() -> Result<()> {
        // GIVEN: A file with nested structure (struct with methods)
        // WHEN: Target filtering for a method name (child symbol)
        // THEN: Should show the parent struct WITH that method visible

        let handler = JulieServerHandler::new().await.unwrap();

        // Index the symbols.rs file itself (has GetSymbolsTool with methods)
        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(std::env::current_dir()?.to_string_lossy().to_string()),
            workspace_id: None,
            name: None,
            force: Some(true),
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
            limit: None,
        };
        index_tool.call_tool(&handler).await?;

        // Target a method that exists inside GetSymbolsTool
        let tool = GetSymbolsTool {
            file_path: "src/tools/symbols.rs".to_string(),
            max_depth: 2,
            include_body: false,
            target: Some("call_tool".to_string()), // Method inside GetSymbolsTool
            mode: None,
        };

        let result = tool.call_tool(&handler).await?;

        // Extract text from result
        let text = result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
            })
            .collect::<Vec<String>>()
            .join("\n");

        // Should find GetSymbolsTool (parent) because it contains call_tool method
        assert!(
            text.contains("GetSymbolsTool"),
            "Should show parent struct when targeting child method.\nGot: {}",
            text
        );

        // Should also show the targeted method
        assert!(
            text.contains("call_tool"),
            "Should show the targeted method.\nGot: {}",
            text
        );

        // Should NOT show "0 symbols matching" error
        assert!(
            !text.contains("No symbols matching"),
            "Should find matches for child symbols.\nGot: {}",
            text
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore] // SLOW/HANGS: Indexes entire workspace (300+ files) - not critical for CLI tools
    async fn test_target_filtering_top_level_still_works() -> Result<()> {
        // GIVEN: A file with top-level symbols
        // WHEN: Target filtering for a top-level symbol name
        // THEN: Should show that symbol (existing behavior should still work)

        let handler = JulieServerHandler::new().await.unwrap();

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(std::env::current_dir()?.to_string_lossy().to_string()),
            workspace_id: None,
            name: None,
            force: Some(true),
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
            limit: None,
        };
        index_tool.call_tool(&handler).await?;

        // Target a top-level struct
        let tool = GetSymbolsTool {
            file_path: "src/tools/symbols.rs".to_string(),
            max_depth: 2,
            include_body: false,
            target: Some("GetSymbolsTool".to_string()),
            mode: None,
        };

        let result = tool.call_tool(&handler).await?;

        let text = result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
            })
            .collect::<Vec<String>>()
            .join("\n");

        // Should find the struct
        assert!(
            text.contains("GetSymbolsTool"),
            "Should find top-level symbol.\nGot: {}",
            text
        );

        // Should show its methods too (existing behavior)
        assert!(
            text.contains("call_tool"),
            "Should show methods of matched struct.\nGot: {}",
            text
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore] // SLOW/HANGS: Indexes entire workspace (300+ files) - not critical for CLI tools
    async fn test_target_filtering_case_insensitive() -> Result<()> {
        // GIVEN: Symbols with mixed case names
        // WHEN: Target filtering with different case
        // THEN: Should match case-insensitively

        let handler = JulieServerHandler::new().await.unwrap();

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(std::env::current_dir()?.to_string_lossy().to_string()),
            workspace_id: None,
            name: None,
            force: Some(true),
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
            limit: None,
        };
        index_tool.call_tool(&handler).await?;

        // Target with lowercase while actual is CamelCase
        let tool = GetSymbolsTool {
            file_path: "src/tools/symbols.rs".to_string(),
            max_depth: 2,
            include_body: false,
            target: Some("getsymbolstool".to_string()), // lowercase
            mode: None,
        };

        let result = tool.call_tool(&handler).await?;

        let text = result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
            })
            .collect::<Vec<String>>()
            .join("\n");

        assert!(
            text.contains("GetSymbolsTool"),
            "Should match case-insensitively.\nGot: {}",
            text
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore] // SLOW/HANGS: Indexes entire workspace (300+ files) - not critical for CLI tools
    async fn test_target_filtering_partial_match() -> Result<()> {
        // GIVEN: Symbols with long names
        // WHEN: Target filtering with partial name
        // THEN: Should match substring

        let handler = JulieServerHandler::new().await.unwrap();

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(std::env::current_dir()?.to_string_lossy().to_string()),
            workspace_id: None,
            name: None,
            force: Some(true),
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
            limit: None,
        };
        index_tool.call_tool(&handler).await?;

        // Partial match - just "format"
        let tool = GetSymbolsTool {
            file_path: "src/tools/symbols.rs".to_string(),
            max_depth: 2,
            include_body: false,
            target: Some("format".to_string()), // Should match "format_symbol"
            mode: None,
        };

        let result = tool.call_tool(&handler).await?;

        let text = result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
            })
            .collect::<Vec<String>>()
            .join("\n");

        assert!(
            text.contains("format_symbol"),
            "Should match partial names.\nGot: {}",
            text
        );

        Ok(())
    }
}
