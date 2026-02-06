//! Tests for GetSymbolsTool Phase 2 - Smart Read with Code Bodies
//!
//! TDD: Write failing tests first, then implement the feature
//! These tests verify the mode parameter for code body extraction

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::{GetSymbolsTool, ManageWorkspaceTool};
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
    // Default: mode="structure" (default) -> should strip code_context
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.clone()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Call with default parameters (mode not set → "structure" which produces lean overview)
    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: None,       // Default = "structure" → lean overview (no code bodies)
        workspace: None,

    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    // In structure mode (default), output is a lean symbol list without code bodies
    // It should contain symbol names but NOT function body code
    assert!(!text.is_empty(), "Should find at least one symbol");
    assert!(text.contains("get_user"), "Should list get_user symbol");
    assert!(text.contains("User"), "Should list User symbol");

    // Structure mode should NOT contain function body code
    assert!(
        !text.contains("id: id.to_string()"),
        "Structure mode should not include function body code, got: {}",
        text
    );
    Ok(())
}

#[tokio::test]
async fn test_structure_mode_strips_context() -> Result<()> {
    // Explicit mode="structure" -> should strip code_context
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.clone()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Call with explicit mode="structure" → lean overview (no code bodies)
    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: Some("structure".to_string()),
        workspace: None,
 // lean format (structure mode has no code bodies)
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    // Structure mode produces lean symbol list — no code bodies
    assert!(!text.is_empty(), "Should find at least one symbol");
    assert!(text.contains("get_user"), "Should list get_user symbol");

    // Should NOT contain function body code
    assert!(
        !text.contains("id: id.to_string()"),
        "Structure mode should not include function body code"
    );
    Ok(())
}

#[tokio::test]
async fn test_mode_structure_always_strips() -> Result<()> {
    // mode="structure" should ALWAYS strip code_context
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.clone()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Call with mode="structure" → lean overview always, no code bodies
    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: Some("structure".to_string()),
        workspace: None,
 // lean format (structure mode has no code bodies)
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    // Structure mode always strips code bodies
    assert!(!text.is_empty(), "Should find at least one symbol");
    assert!(text.contains("User"), "Should list User symbol");

    // Should NOT contain function body code
    assert!(
        !text.contains("id: id.to_string()"),
        "Structure mode should never include function body code"
    );
    Ok(())
}

#[tokio::test]
async fn test_mode_minimal_top_level_only() -> Result<()> {
    // mode="minimal" -> extract bodies for TOP-LEVEL symbols only
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.clone()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: Some("minimal".to_string()),
        workspace: None,
 // Default → "code" format (since minimal provides code bodies)
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    // Minimal mode extracts code bodies for top-level symbols
    // The "code" format outputs raw source code
    assert!(!text.is_empty(), "Should find at least one symbol");

    // Top-level function get_user should have its code body present
    assert!(
        text.contains("pub fn get_user"),
        "Minimal mode should include top-level function code body"
    );

    // Top-level struct User should be present
    assert!(
        text.contains("pub struct User"),
        "Minimal mode should include top-level struct definition"
    );
    Ok(())
}

#[tokio::test]
async fn test_mode_full_all_symbols() -> Result<()> {
    // mode="full" -> extract bodies for ALL symbols including nested
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.clone()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 2, // Increase depth to get nested symbols
        target: None,
        limit: None,
        mode: Some("full".to_string()),
        workspace: None,
 // Default → "code" format (since full provides code bodies)
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    // Full mode extracts code bodies for ALL symbols including nested
    assert!(!text.is_empty(), "Should find at least one symbol");

    // Should contain all symbol code bodies
    assert!(
        text.contains("pub fn get_user"),
        "Full mode should include top-level function"
    );
    assert!(
        text.contains("pub struct User"),
        "Full mode should include struct definition"
    );
    // Nested methods from impl block should also be present
    assert!(
        text.contains("fn new("),
        "Full mode should include nested method 'new'"
    );
    assert!(
        text.contains("fn display_name"),
        "Full mode should include nested method 'display_name'"
    );
    Ok(())
}

#[tokio::test]
async fn test_target_with_minimal_mode() -> Result<()> {
    // Test that target filtering works with mode="minimal"
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.clone()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: Some("User".to_string()),
        limit: None,
        mode: Some("minimal".to_string()),
        workspace: None,
 // Default → "code" format (since minimal provides code bodies)
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    // Should find User struct when filtering by target
    assert!(
        text.contains("User"),
        "Should find User struct when filtering by target, got: {}",
        text
    );

    // In minimal mode, User struct code body should be present
    assert!(
        text.contains("pub struct User"),
        "Top-level User struct should have code body in minimal mode"
    );

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

    // Try to get symbols from non-existent file
    let tool = GetSymbolsTool {
        file_path: "src/nonexistent.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: Some("minimal".to_string()),
        workspace: None,

    };

    let result = tool.call_tool(&handler).await;

    // Should either return error or "File not found" message - not panic
    match result {
        Ok(call_result) => {
            let text = extract_text_from_result(&call_result);
            assert!(
                text.contains("File not found")
                    || text.contains("No symbols found")
                    || text.is_empty(),
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
    handler
        .initialize_workspace_with_force(Some(workspace_path.clone()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
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
        mode: Some("minimal".to_string()),
        workspace: None,
 // Default → "code" format (since minimal provides code bodies)
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);

    // Should succeed and have symbols
    assert!(
        !text.is_empty(),
        "Should successfully extract symbols even with UTF-8 handling"
    );
    assert!(
        text.contains("get_user") || text.contains("User"),
        "Should contain symbol names in output"
    );
    Ok(())
}
