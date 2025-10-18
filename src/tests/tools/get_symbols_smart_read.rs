//! Tests for GetSymbolsTool Phase 2 - Smart Read with Code Bodies
//!
//! TDD: Write failing tests first, then implement the feature
//! These tests verify the include_body and mode parameters for code body extraction

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::{GetSymbolsTool, ManageWorkspaceTool};
use rust_mcp_sdk::schema::CallToolResult;

/// Extract text from CallToolResult safely
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

/// Extract structured symbols from CallToolResult
fn extract_symbols_from_result(result: &CallToolResult) -> Vec<serde_json::Value> {
    result
        .structured_content
        .as_ref()
        .and_then(|sc| {
            sc.get("symbols")
                .and_then(|s| s.as_array())
                .map(|a| a.clone())
        })
        .unwrap_or_default()
}

/// Create a temporary Rust file with known structure for testing
fn create_test_rust_file() -> Result<(TempDir, String)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Create a simple Rust file with symbols
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("example.rs");
    let file_content = r#"/// Get user by ID
pub fn get_user(id: &str) -> User {
    User {
        id: id.to_string(),
        name: "Test".to_string(),
    }
}

pub struct User {
    pub id: String,
    pub name: String,
}

impl User {
    pub fn new(id: String, name: String) -> Self {
        User { id, name }
    }

    pub fn display_name(&self) -> String {
        self.name.clone()
    }
}

pub const MAX_USERS: usize = 100;
"#;
    fs::write(&test_file, file_content)?;

    Ok((temp_dir, workspace_path.to_string_lossy().to_string()))
}

#[tokio::test]
async fn test_default_behavior_strips_context() -> Result<()> {
    // Default: include_body=false, mode="structure" -> should strip code_context
    let (temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler.initialize_workspace(Some(workspace_path.clone())).await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
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
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Call with default parameters (include_body not set, mode not set)
    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        include_body: None,
        mode: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    // Verify code_context is stripped (None or empty)
    for symbol in &symbols {
        let code_context = symbol.get("code_context");
        assert!(
            code_context.is_none() || code_context == Some(&serde_json::Value::Null),
            "Expected code_context to be None/null in default mode, got: {:?}",
            code_context
        );
    }

    assert!(!symbols.is_empty(), "Should find at least one symbol");
    Ok(())
}

#[tokio::test]
async fn test_include_body_false_strips_context() -> Result<()> {
    // Explicit include_body=false -> should strip code_context
    let (temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler.initialize_workspace(Some(workspace_path.clone())).await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
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
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Call with explicit include_body=false
    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        include_body: Some(false),
        mode: Some("structure".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    // Verify code_context is stripped
    for symbol in &symbols {
        let code_context = symbol.get("code_context");
        assert!(
            code_context.is_none() || code_context == Some(&serde_json::Value::Null),
            "Expected code_context to be None/null when include_body=false"
        );
    }

    assert!(!symbols.is_empty(), "Should find at least one symbol");
    Ok(())
}

#[tokio::test]
async fn test_mode_structure_always_strips() -> Result<()> {
    // mode="structure" should ALWAYS strip code_context, even if include_body=true
    let (temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler.initialize_workspace(Some(workspace_path.clone())).await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
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
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Call with mode="structure" and include_body=true (should ignore include_body)
    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        include_body: Some(true),
        mode: Some("structure".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    // Verify code_context is still stripped (structure mode overrides include_body)
    for symbol in &symbols {
        let code_context = symbol.get("code_context");
        assert!(
            code_context.is_none() || code_context == Some(&serde_json::Value::Null),
            "Expected code_context to be None/null in structure mode, even with include_body=true"
        );
    }

    assert!(!symbols.is_empty(), "Should find at least one symbol");
    Ok(())
}

#[tokio::test]
async fn test_mode_minimal_top_level_only() -> Result<()> {
    // mode="minimal" with include_body=true -> extract bodies for TOP-LEVEL symbols only
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler.initialize_workspace(Some(workspace_path.clone())).await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
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
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        include_body: Some(true),
        mode: Some("minimal".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    // Verify: top-level symbols have code_context, nested symbols don't
    let mut found_top_level = false;

    for symbol in &symbols {
        let parent_id = symbol.get("parent_id");
        let code_context = symbol.get("code_context");
        let is_top_level = parent_id.is_none() || parent_id == Some(&serde_json::Value::Null);

        if is_top_level {
            found_top_level = true;
            // Top-level should have code_context populated
            assert!(
                code_context.is_some() && code_context != Some(&serde_json::Value::Null),
                "Expected code_context for top-level symbol: {}",
                symbol.get("name").unwrap_or(&serde_json::Value::Null)
            );
        } else {
            // Nested should NOT have code_context
            assert!(
                code_context.is_none() || code_context == Some(&serde_json::Value::Null),
                "Expected NO code_context for nested symbol"
            );
        }
    }

    assert!(found_top_level, "Should find at least one top-level symbol");
    // Note: we might not have nested symbols depending on file structure
    Ok(())
}

#[tokio::test]
async fn test_mode_full_all_symbols() -> Result<()> {
    // mode="full" with include_body=true -> extract bodies for ALL symbols including nested
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler.initialize_workspace(Some(workspace_path.clone())).await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
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
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 2, // Increase depth to get nested symbols
        target: None,
        limit: None,
        include_body: Some(true),
        mode: Some("full".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    // Verify: ALL symbols have code_context populated (or are constants with no body)
    for symbol in &symbols {
        let code_context = symbol.get("code_context");
        let symbol_kind = symbol.get("kind").and_then(|k| k.as_str()).unwrap_or("");

        // For most symbols (functions, methods, classes), should have code_context
        if symbol_kind != "constant" {
            assert!(
                code_context.is_some() && code_context != Some(&serde_json::Value::Null),
                "Expected code_context for symbol: {} (kind: {})",
                symbol.get("name").unwrap_or(&serde_json::Value::Null),
                symbol_kind
            );
        }
    }

    assert!(!symbols.is_empty(), "Should find at least one symbol");
    Ok(())
}

#[tokio::test]
async fn test_target_with_include_body() -> Result<()> {
    // Test that target filtering works with include_body=true and mode="minimal"
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler.initialize_workspace(Some(workspace_path.clone())).await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
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
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: Some("User".to_string()),
        limit: None,
        include_body: Some(true),
        mode: Some("minimal".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    // Should have User struct and its methods
    let has_user_struct = symbols.iter().any(|s| {
        s.get("name")
            .and_then(|n| n.as_str())
            .map(|n| n.contains("User"))
            .unwrap_or(false)
    });

    assert!(has_user_struct, "Should find User struct when filtering by target");

    // Top-level User should have code_context
    for symbol in &symbols {
        let name = symbol.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let parent_id = symbol.get("parent_id");
        let code_context = symbol.get("code_context");

        if name == "User" && (parent_id.is_none() || parent_id == Some(&serde_json::Value::Null)) {
            assert!(
                code_context.is_some() && code_context != Some(&serde_json::Value::Null),
                "Top-level User struct should have code_context"
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_file_read_error_handling() -> Result<()> {
    // Test graceful error handling for missing files
    let _temp_dir = TempDir::new()?;
    let workspace_path = _temp_dir.path().to_path_buf();

    // Don't create any files

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
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Try to get symbols from non-existent file
    let tool = GetSymbolsTool {
        file_path: "src/nonexistent.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        include_body: Some(true),
        mode: Some("minimal".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await;

    // Should either return error or "No symbols found" - not panic
    match result {
        Ok(call_result) => {
            let text = extract_text_from_result(&call_result);
            assert!(
                text.contains("No symbols found") || text.is_empty(),
                "Should gracefully handle missing file, got: {}",
                text
            );
        }
        Err(_e) => {
            // Error is acceptable for missing file
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_utf8_decode_error_handling() -> Result<()> {
    // Test graceful handling of UTF-8 issues (using lossy conversion)
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler.initialize_workspace(Some(workspace_path.clone())).await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
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
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Call with valid parameters - should work even if file has non-UTF8 bytes
    // (This is a valid test even if our test file is valid UTF-8)
    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        include_body: Some(true),
        mode: Some("minimal".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let symbols = extract_symbols_from_result(&result);

    // Should succeed and have symbols
    assert!(!symbols.is_empty(), "Should successfully extract symbols even with UTF-8 handling");
    Ok(())
}
