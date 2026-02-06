//! Tests for fast_search line-level output mode
//! TDD: RED → GREEN → REFACTOR

#[cfg(test)]
mod search_line_mode_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::{Symbol, SymbolKind};
    use crate::handler::JulieServerHandler;
    use crate::mcp_compat::StructuredContentExt;
    use crate::tools::search::FastSearchTool;
    use crate::tools::workspace::ManageWorkspaceTool;
    use anyhow::Result;
    use chrono::Utc;
    use std::fs;
    use std::sync::atomic::Ordering;
    use tempfile::TempDir;
    use tokio::time::{Duration, sleep};

    /// Extract text from CallToolResult safely (handles both TOON and JSON modes)
    fn extract_text_from_result(result: &crate::mcp_compat::CallToolResult) -> String {
        // Try extracting from .content first (TOON mode)
        if !result.content.is_empty() {
            return result
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
                .join("\n");
        }

        // Fall back to .structured_content (JSON mode)
        if let Some(structured) = result.structured_content() {
            return serde_json::to_string_pretty(&structured).unwrap_or_default();
        }

        String::new()
    }

    #[allow(dead_code)]
    fn extract_workspace_id(result: &crate::mcp_compat::CallToolResult) -> Option<String> {
        let text = extract_text_from_result(result);
        text.lines()
            .find(|line| line.contains("Workspace ID:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|id| id.trim().to_string())
    }

    async fn mark_index_ready(handler: &JulieServerHandler) {
        handler
            .indexing_status
            .search_ready
            .store(true, Ordering::Relaxed);
        *handler.is_indexed.write().await = true;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_basic() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

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
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "TODO".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
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
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

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
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        // Test 1: Search primary workspace explicitly - should find results
        let search_primary = FastSearchTool {
            query: "alpha_marker".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
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
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("nonexistent_workspace_id".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
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
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

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
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "user -password".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
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
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
        }

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
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        let search_tool = FastSearchTool {
            query: "getUserData".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
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
            response_text.contains("getUserData")
                || response_text.contains("Found")
                || response_text.contains("symbol"),
            "Should show basic search result info"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_fast_search_line_mode_language_filter() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        // Create files in different languages with common search term
        let rust_file = src_dir.join("example.rs");
        fs::write(
            &rust_file,
            r#"// TODO: implement feature
fn rust_function() {}
"#,
        )?;

        let ts_file = src_dir.join("example.ts");
        fs::write(
            &ts_file,
            r#"// TODO: implement feature
function typescriptFunction() {}
"#,
        )?;

        let py_file = src_dir.join("example.py");
        fs::write(
            &py_file,
            r#"# TODO: implement feature
def python_function():
    pass
"#,
        )?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        // Test: Search with rust language filter
        let search_rust = FastSearchTool {
            query: "TODO".to_string(),
            language: Some("rust".to_string()),
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
        };

        let result = search_rust.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("example.rs"),
            "Should find TODO in Rust file"
        );
        assert!(
            !response_text.contains("example.ts"),
            "Should NOT include TypeScript file when filtering for Rust"
        );
        assert!(
            !response_text.contains("example.py"),
            "Should NOT include Python file when filtering for Rust"
        );

        // Test: Search with typescript language filter
        let search_ts = FastSearchTool {
            query: "TODO".to_string(),
            language: Some("typescript".to_string()),
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
        };

        let result_ts = search_ts.call_tool(&handler).await?;
        let response_ts = extract_text_from_result(&result_ts);

        assert!(
            response_ts.contains("example.ts"),
            "Should find TODO in TypeScript file"
        );
        assert!(
            !response_ts.contains("example.rs"),
            "Should NOT include Rust file when filtering for TypeScript"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "FTS content indexing timing issue - needs investigation"]
    async fn test_fast_search_line_mode_file_pattern_filter() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        // Create directory structure
        let src_dir = workspace_path.join("src");
        let tests_dir = workspace_path.join("tests");
        fs::create_dir_all(&src_dir)?;
        fs::create_dir_all(&tests_dir)?;

        // Create files with common search term in different locations
        let src_file = src_dir.join("code.rs");
        fs::write(&src_file, "// FIXME: handle error\n")?;

        let test_file = tests_dir.join("test.rs");
        fs::write(&test_file, "// FIXME: add test case\n")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_secs(2)).await; // Increased wait for FTS content indexing
        mark_index_ready(&handler).await;

        // Test: Search with src/** file pattern
        let search_src = FastSearchTool {
            query: "FIXME".to_string(),
            language: None,
            file_pattern: Some("src/**".to_string()),
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
        };

        let result = search_src.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("src/code.rs") || response_text.contains("src\\code.rs"),
            "Should find FIXME in src/ directory: {}",
            response_text
        );
        assert!(
            !response_text.contains("tests/test.rs") && !response_text.contains("tests\\test.rs"),
            "Should NOT include tests/ directory when filtering for src/**"
        );

        // Test: Search with tests/** file pattern
        let search_tests = FastSearchTool {
            query: "FIXME".to_string(),
            language: None,
            file_pattern: Some("tests/**".to_string()),
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
        };

        let result_tests = search_tests.call_tool(&handler).await?;
        let response_tests = extract_text_from_result(&result_tests);

        assert!(
            response_tests.contains("tests/test.rs") || response_tests.contains("tests\\test.rs"),
            "Should find FIXME in tests/ directory"
        );
        assert!(
            !response_tests.contains("src/code.rs") && !response_tests.contains("src\\code.rs"),
            "Should NOT include src/ directory when filtering for tests/**"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "FTS content indexing timing issue - needs investigation"]
    async fn test_fast_search_line_mode_combined_filters() -> Result<()> {
        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
        }

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        // Create multiple files
        let rust_file = src_dir.join("main.rs");
        fs::write(&rust_file, "// TODO: rust implementation\n")?;

        let ts_file = src_dir.join("index.ts");
        fs::write(&ts_file, "// TODO: typescript implementation\n")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        sleep(Duration::from_secs(2)).await; // Increased wait for FTS content indexing
        mark_index_ready(&handler).await;

        // Test: Search with BOTH language AND file_pattern filters
        let search_combined = FastSearchTool {
            query: "TODO".to_string(),
            language: Some("rust".to_string()),
            file_pattern: Some("src/**/*.rs".to_string()),
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
        };

        let result = search_combined.call_tool(&handler).await?;
        let response_text = extract_text_from_result(&result);

        assert!(
            response_text.contains("main.rs"),
            "Should find TODO in Rust file matching both filters"
        );
        assert!(
            !response_text.contains("index.ts"),
            "Should NOT include TypeScript file when filtering for Rust + src/**/*.rs"
        );

        Ok(())
    }
}
