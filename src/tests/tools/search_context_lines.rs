//! Tests for FastSearchTool context_lines parameter - verify token-efficient context truncation
//!
//! TDD: Test the context_lines parameter to ensure code_context is truncated properly
//! This saves massive tokens (10x-30x) in search results while maintaining usefulness

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::tools::{FastSearchTool, ManageWorkspaceTool};
use rust_mcp_sdk::schema::CallToolResult;

/// Extract structured content as symbols from CallToolResult
fn extract_symbols_from_result(result: &CallToolResult) -> Vec<Symbol> {
    result
        .structured_content
        .as_ref()
        .and_then(|map| map.get("results"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(Vec::new)
}

#[tokio::test(flavor = "multi_thread")]
async fn test_context_lines_default_behavior() -> Result<()> {
    // Test default context_lines=1 means 3 total lines (1 before + match + 1 after)

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create a file with a function that has lots of context
    let test_file = src_dir.join("example.rs");
    fs::write(
        &test_file,
        r#"
// Line 1: Comment before
// Line 2: Another comment
pub fn process_user_data(input: &str) -> String {
    // Line 4: Inside function
    // Line 5: More code
    // Line 6: Even more
    // Line 7: And more
    // Line 8: Last line
    input.to_uppercase()
}
// Line 11: After function
// Line 12: More after
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Index the workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Search with DEFAULT context_lines (should be 1 = 3 total lines)
    let tool = FastSearchTool {
        query: "process_user_data".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: Some("primary".to_string()),
        search_target: "definitions".to_string(),
        output: Some("symbols".to_string()),
        context_lines: None, // Use default (1)
        output_format: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    assert!(!symbols.is_empty(), "Should find the function");

    // Find the process_user_data symbol
    let symbol = symbols
        .iter()
        .find(|s| s.name == "process_user_data")
        .expect("Should find process_user_data function");

    if let Some(code_context) = &symbol.code_context {
        let lines: Vec<&str> = code_context.lines().collect();

        // Default context_lines=1 means max 3 lines (1 before + match + 1 after)
        // If original context was longer, should be truncated to 3 lines + "..."
        if lines.len() > 3 {
            // Should be truncated
            let last_line = lines.last().unwrap();
            assert!(
                last_line.contains("..."),
                "Should have truncation indicator, got: {:?}",
                lines
            );
            // Should have at most 4 lines (3 + "..." line)
            assert!(
                lines.len() <= 4,
                "Should have at most 4 lines (3 + ...), got {}",
                lines.len()
            );
        }
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_context_lines_zero() -> Result<()> {
    // Test context_lines=0 means 1 total line (just the match)

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("example.rs");
    fs::write(
        &test_file,
        r#"
// Before
pub fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}
// After
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
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

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Search with context_lines=0 (just match line, 1 total)
    let tool = FastSearchTool {
        query: "calculate_sum".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: Some("primary".to_string()),
        search_target: "definitions".to_string(),
        output: Some("symbols".to_string()),
        context_lines: Some(0), // 0 = just match line
        output_format: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    assert!(!symbols.is_empty(), "Should find the function");

    let symbol = symbols
        .iter()
        .find(|s| s.name == "calculate_sum")
        .expect("Should find calculate_sum function");

    if let Some(code_context) = &symbol.code_context {
        let lines: Vec<&str> = code_context.lines().collect();

        // context_lines=0 means max 1 line
        // If truncated, should have "..." indicator
        if lines.len() > 1 {
            let last_line = lines.last().unwrap();
            assert!(
                last_line.contains("..."),
                "Should have truncation indicator for context_lines=0"
            );
            assert!(
                lines.len() <= 2,
                "Should have at most 2 lines (1 + ...) for context_lines=0, got {}",
                lines.len()
            );
        }
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_context_lines_grep_default() -> Result<()> {
    // Test context_lines=3 means 7 total lines (3 before + match + 3 after) - grep default

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("example.rs");
    fs::write(
        &test_file,
        r#"
// Line 1
// Line 2
// Line 3
pub fn validate_input(data: &str) -> bool {
    // Line 5
    // Line 6
    // Line 7
    // Line 8
    // Line 9
    !data.is_empty()
}
// Line 12
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
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

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Search with context_lines=3 (grep default: 3 before + match + 3 after = 7 total)
    let tool = FastSearchTool {
        query: "validate_input".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: Some("primary".to_string()),
        search_target: "definitions".to_string(),
        output: Some("symbols".to_string()),
        context_lines: Some(3), // 3 = grep default (7 total lines)
        output_format: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    assert!(!symbols.is_empty(), "Should find the function");

    let symbol = symbols
        .iter()
        .find(|s| s.name == "validate_input")
        .expect("Should find validate_input function");

    if let Some(code_context) = &symbol.code_context {
        let lines: Vec<&str> = code_context.lines().collect();

        // context_lines=3 means max 7 lines
        // If truncated, should have "..." indicator
        if lines.len() > 7 {
            let last_line = lines.last().unwrap();
            assert!(
                last_line.contains("..."),
                "Should have truncation indicator for context_lines=3"
            );
            assert!(
                lines.len() <= 8,
                "Should have at most 8 lines (7 + ...) for context_lines=3, got {}",
                lines.len()
            );
        }
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_context_short_not_truncated() -> Result<()> {
    // Test that code_context shorter than max_lines is NOT truncated

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("example.rs");
    fs::write(
        &test_file,
        r#"
pub fn short_func() -> i32 { 42 }
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
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

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Search with default context_lines (3 total lines max)
    let tool = FastSearchTool {
        query: "short_func".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: Some("primary".to_string()),
        search_target: "definitions".to_string(),
        output: Some("symbols".to_string()),
        context_lines: None, // Default (1)
        output_format: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    assert!(!symbols.is_empty(), "Should find the function");

    let symbol = symbols
        .iter()
        .find(|s| s.name == "short_func")
        .expect("Should find short_func function");

    if let Some(code_context) = &symbol.code_context {
        // Should NOT have "..." since context is short
        assert!(
            !code_context.contains("..."),
            "Short context should NOT be truncated: {}",
            code_context
        );
    }

    Ok(())
}
