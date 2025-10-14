//! Tests for fast_search line-level output mode
//! TDD: RED → GREEN → REFACTOR

#[cfg(test)]
mod search_line_mode_tests {
    use crate::handler::JulieServerHandler;
    use crate::tools::search::FastSearchTool;
    use crate::tools::workspace::ManageWorkspaceTool;
    use anyhow::Result;
    use std::fs;
    use tempfile::TempDir;

    /// Extract text from CallToolResult safely
    fn extract_text_from_result(result: &rust_mcp_sdk::schema::CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_basic() -> Result<()> {
        // TDD RED: This test WILL FAIL because line mode doesn't exist yet
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        // Create test file with known content
        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        let test_file = src_dir.join("example.rs");
        fs::write(
            &test_file,
            r#"// TODO: implement authentication
fn getUserData() {
    // TODO: add validation
    println!("Getting user data");
}

fn processPayment() {
    // This function is complete
    println!("Processing payment");
}
"#,
        )?;

        // Initialize handler and index
        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
            .await?;

        // Index the workspace
        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        // Wait for indexing
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Search for TODO comments with line mode
        let search_tool = FastSearchTool {
            query: "TODO".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            output: Some("lines".to_string()), // NEW PARAMETER
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        // Should find both TODO lines
        assert!(
            response_text.contains("TODO: implement authentication"),
            "Should find first TODO comment"
        );
        assert!(
            response_text.contains("TODO: add validation"),
            "Should find second TODO comment"
        );

        // Should include line numbers
        assert!(
            response_text.contains("Line 1") || response_text.contains(":1:"),
            "Should include line number for first TODO"
        );

        // Should NOT include processPayment line (doesn't contain TODO)
        assert!(
            !response_text.contains("Processing payment"),
            "Should NOT include lines without TODO"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_symbols_mode_default() -> Result<()> {
        // TDD: Verify that default mode still returns symbols, not lines
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        let test_file = src_dir.join("example.rs");
        fs::write(
            &test_file,
            r#"pub fn getUserData() -> User {
    User { name: "test" }
}
"#,
        )?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Search WITHOUT output parameter (should default to symbols)
        let search_tool = FastSearchTool {
            query: "getUserData".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            output: None, // Default should be symbols mode
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        // Should return symbol-level results (function definition)
        assert!(
            response_text.contains("getUserData"),
            "Should find function symbol"
        );
        assert!(
            response_text.contains("[rust]") || response_text.contains("Function"),
            "Should show symbol type info"
        );

        Ok(())
    }
}
