//! Tests for FastSearchTool context_lines parameter - verify token-efficient context truncation
//!
//! TDD: Test the context_lines parameter to ensure code_context is truncated properly
//! This saves massive tokens (10x-30x) in search results while maintaining usefulness

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::{FastSearchTool, ManageWorkspaceTool};
use crate::mcp_compat::CallToolResult;

/// Extract text from CallToolResult content blocks
fn extract_text_from_result(result: &CallToolResult) -> String {
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

/// Count the context lines shown for a match in lean format output.
/// Lean format looks like:
///   src/file.rs:42
///     41: context before
///     42→ matched line
///     43: context after
///
/// Returns the number of indented code context lines following the file:line header.
fn count_context_lines_for_match(text: &str, function_name: &str) -> Option<usize> {
    let lines: Vec<&str> = text.lines().collect();
    // Find a file:line header that's followed by context containing function_name
    for (i, line) in lines.iter().enumerate() {
        // Look for indented context lines containing the function name
        if line.starts_with("  ") && line.contains(function_name) {
            // Count all indented context lines in this block (going back to header)
            let mut start = i;
            while start > 0 && lines[start - 1].starts_with("  ") {
                start -= 1;
            }
            let mut end = i + 1;
            while end < lines.len() && lines[end].starts_with("  ") {
                end += 1;
            }
            return Some(end - start);
        }
    }
    None
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
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: Some("primary".to_string()),
        search_target: "definitions".to_string(),
        context_lines: None, // Use default (1)
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    assert!(
        text.contains("process_user_data"),
        "Should find the function, got: {}",
        text
    );

    // Default context_lines=1 → max 3 lines of context (1 before + match + 1 after)
    // The lean output shows indented context lines under each file:line header
    if let Some(context_count) = count_context_lines_for_match(&text, "process_user_data") {
        // Default context_lines=1 means at most 3 visible context lines
        // (may include "..." truncation as part of the context text)
        assert!(
            context_count <= 4,
            "Default context_lines=1 should show at most ~3 lines + truncation, got {}",
            context_count
        );
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
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: Some("primary".to_string()),
        search_target: "definitions".to_string(),
        context_lines: Some(0), // 0 = just match line
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    assert!(
        text.contains("calculate_sum"),
        "Should find the function, got: {}",
        text
    );

    // context_lines=0 → max 1 line of context (just the match line)
    if let Some(context_count) = count_context_lines_for_match(&text, "calculate_sum") {
        // context_lines=0 means at most 1 visible context line (+ possible truncation marker)
        assert!(
            context_count <= 2,
            "context_lines=0 should show at most 1 line + truncation, got {}",
            context_count
        );
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
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: Some("primary".to_string()),
        search_target: "definitions".to_string(),
        context_lines: Some(3), // 3 = grep default (7 total lines)
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    assert!(
        text.contains("validate_input"),
        "Should find the function, got: {}",
        text
    );

    // context_lines=3 → max 7 lines of context (3 before + match + 3 after)
    if let Some(context_count) = count_context_lines_for_match(&text, "validate_input") {
        // context_lines=3 means at most 7 visible context lines (+ possible truncation marker)
        assert!(
            context_count <= 8,
            "context_lines=3 should show at most 7 lines + truncation, got {}",
            context_count
        );
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
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: Some("primary".to_string()),
        search_target: "definitions".to_string(),
        context_lines: None, // Default (1)
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    assert!(
        text.contains("short_func"),
        "Should find the function, got: {}",
        text
    );

    // Short context should NOT be truncated (no "..." indicator)
    // The lean format puts context as indented lines under the file:line header
    assert!(
        !text.contains("..."),
        "Short context should NOT be truncated: {}",
        text
    );

    Ok(())
}
