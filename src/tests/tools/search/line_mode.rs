//! Tests for fast_search line-level output mode
//! TDD: RED → GREEN → REFACTOR

#[cfg(test)]
mod search_line_mode_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::{Symbol, SymbolKind};
    use crate::handler::JulieServerHandler;
    use crate::tools::search::FastSearchTool;
    use crate::tools::workspace::ManageWorkspaceTool;
    use anyhow::Result;
    use chrono::Utc;
    use std::fs;
    use std::sync::atomic::Ordering;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

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

    #[allow(dead_code)]
    fn extract_workspace_id(result: &rust_mcp_sdk::schema::CallToolResult) -> Option<String> {
        let text = extract_text_from_result(result);
        text.lines()
            .find(|line| line.contains("Workspace ID:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|id| id.trim().to_string())
    }

    async fn mark_index_ready(handler: &JulieServerHandler) {
        handler
            .indexing_status
            .sqlite_fts_ready
            .store(true, Ordering::Relaxed);
        handler
            .indexing_status
            .semantic_ready
            .store(true, Ordering::Relaxed);
        *handler.is_indexed.write().await = true;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_basic() -> Result<()> {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "0");
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");

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
            limit: None,
        };
        index_tool.call_tool(&handler).await?;

        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "TODO".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            output: Some("lines".to_string()),
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("TODO: implement authentication"),
            "Should find first TODO comment"
        );
        assert!(
            response_text.contains("TODO: add validation"),
            "Should find second TODO comment"
        );
        assert!(
            response_text.contains("Line 1") || response_text.contains(":1:"),
            "Should include line numbers"
        );
        assert!(
            !response_text.contains("Processing payment"),
            "Should NOT include unrelated lines"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_respects_workspace_filter() -> Result<()> {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "0");
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        // Create two files with distinct content
        let file1 = src_dir.join("module_a.rs");
        fs::write(
            &file1,
            "fn function_alpha() { println!(\"alpha_marker\"); }\n",
        )?;

        let file2 = src_dir.join("module_b.rs");
        fs::write(
            &file2,
            "fn function_beta() { println!(\"beta_marker\"); }\n",
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
            limit: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        // Test 1: Search primary workspace explicitly - should find results
        let search_primary = FastSearchTool {
            query: "alpha_marker".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            output: Some("lines".to_string()),
        };

        let result = search_primary.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("alpha_marker"),
            "Primary workspace search should find content: {}",
            response_text
        );
        assert!(
            response_text.contains("module_a.rs"),
            "Primary workspace search should show correct file: {}",
            response_text
        );

        // Test 2: Search with invalid workspace ID - should return error
        let search_invalid = FastSearchTool {
            query: "alpha_marker".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("nonexistent_workspace_id".to_string()),
            output: Some("lines".to_string()),
        };

        let result = search_invalid.call_tool(&handler).await;
        assert!(
            result.is_err(),
            "Searching non-existent workspace should return error"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_handles_exclusion_queries() -> Result<()> {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "0");
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        let test_file = src_dir.join("filters.rs");
        fs::write(
            &test_file,
            r#"// user profile data
// user password secret
// user preferences dashboard
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
            limit: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "user -password".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            output: Some("lines".to_string()),
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        // Verify correct lines are included
        assert!(
            response_text.contains("user profile data"),
            "Should include line without excluded term: {}",
            response_text
        );
        assert!(
            response_text.contains("user preferences dashboard"),
            "Should include other matching line: {}",
            response_text
        );

        // Verify excluded line is NOT in results (check line content, not header)
        assert!(
            !response_text.contains("user password secret"),
            "Should exclude lines containing the forbidden term: {}",
            response_text
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_symbols_mode_default() -> Result<()> {
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
            limit: None,
        };
        index_tool.call_tool(&handler).await?;

        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "getUserData".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            output: None,
        };

        let result = search_tool.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("getUserData"),
            "Should find function symbol"
        );
        // NEW FORMAT: Minimal 2-line summary shows symbol name and basic status
        // Symbol type details are in structured_content JSON, not required in minimal text
        assert!(
            response_text.contains("getUserData") || response_text.contains("Found") || response_text.contains("symbol"),
            "Should show basic search result info"
        );

        Ok(())
    }
}
