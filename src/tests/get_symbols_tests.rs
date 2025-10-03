//! Tests for GetSymbolsTool - verify path normalization and symbol retrieval
//!
//! TDD: Write failing tests first, then fix the implementation

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

#[tokio::test]
async fn test_get_symbols_with_relative_path() -> Result<()> {
    // TDD: This test WILL FAIL initially because get_symbols doesn't normalize paths
    //
    // BUG: Database stores absolute paths like /tmp/workspace/src/main.rs
    //      but get_symbols queries with relative path like src/main.rs
    //      Result: "No symbols found" error even though symbols exist

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Create a simple Rust file with symbols
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("example.rs");
    fs::write(
        &test_file,
        r#"
pub struct User {
    pub id: String,
    pub name: String,
}

pub fn get_user(id: &str) -> User {
    User {
        id: id.to_string(),
        name: "Test".to_string(),
    }
}

pub const MAX_USERS: usize = 100;
"#,
    )?;

    // Initialize handler and index the workspace
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
        .await?;

    // Explicitly trigger indexing (initialize_workspace doesn't auto-index)
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
    let index_result = index_tool.call_tool(&handler).await?;
    println!(
        "DEBUG: Indexing result: {:?}",
        extract_text_from_result(&index_result)
    );

    // Wait a moment for indexing to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // DEBUG: Check what path the database has AND what the file actually is
    println!("DEBUG: Test file created at: {}", test_file.display());
    println!("DEBUG: Test file exists: {}", test_file.exists());

    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace should exist");
    let db = workspace.db.as_ref().expect("DB should exist");
    let db_lock = db.lock().await;
    let all_symbols = db_lock.get_all_symbols().expect("Should get symbols");
    println!("DEBUG: Found {} symbols in database", all_symbols.len());
    if let Some(first_symbol) = all_symbols.first() {
        println!("DEBUG: First symbol file_path: {}", first_symbol.file_path);
    } else {
        println!("DEBUG: No symbols found - indexing may have failed");
    }
    drop(db_lock);

    // Query using RELATIVE path (this should work but currently fails!)
    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(), // RELATIVE path
        max_depth: 2,
        include_body: false,
        target: None,
        mode: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text_content = extract_text_from_result(&result);

    // Should find symbols, not return "No symbols found" error
    assert!(
        !text_content.contains("No symbols found"),
        "Expected to find symbols but got: {}",
        text_content
    );
    assert!(
        text_content.contains("User"),
        "Should find User struct in symbols"
    );
    assert!(
        text_content.contains("get_user"),
        "Should find get_user function in symbols"
    );
    assert!(
        text_content.contains("MAX_USERS"),
        "Should find MAX_USERS constant in symbols"
    );

    Ok(())
}

#[tokio::test]
async fn test_get_symbols_with_absolute_path() -> Result<()> {
    // This test should PASS even before the fix because absolute paths work

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("example.rs");
    fs::write(
        &test_file,
        r#"
pub fn process_data(input: &str) -> String {
    input.to_uppercase()
}
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
        .await?;

    // Explicitly trigger indexing
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

    // Query using ABSOLUTE path (should work even before fix)
    let absolute_path = test_file.to_string_lossy().to_string();
    let tool = GetSymbolsTool {
        file_path: absolute_path,
        max_depth: 1,
        include_body: false,
        target: None,
        mode: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text_content = extract_text_from_result(&result);

    assert!(
        !text_content.contains("No symbols found"),
        "Should find symbols with absolute path"
    );
    assert!(
        text_content.contains("process_data"),
        "Should find process_data function"
    );

    Ok(())
}

#[tokio::test]
async fn test_get_symbols_normalizes_various_path_formats() -> Result<()> {
    // Test that various path formats all work correctly after normalization

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("utils.rs");
    fs::write(&test_file, "pub fn helper() -> i32 { 42 }")?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
        .await?;

    // Explicitly trigger indexing
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

    // Test various path formats - all should work
    let path_variants = vec![
        "src/utils.rs",        // Simple relative
        "./src/utils.rs",      // Relative with ./
        "src/../src/utils.rs", // Relative with .. (should normalize)
    ];

    for path_variant in path_variants {
        let tool = GetSymbolsTool {
            file_path: path_variant.to_string(),
            max_depth: 1,
            include_body: false,
            target: None,
            mode: None,
        };

        let result = tool.call_tool(&handler).await?;
        let text_content = extract_text_from_result(&result);

        assert!(
            !text_content.contains("No symbols found"),
            "Path variant '{}' should work but got: {}",
            path_variant,
            text_content
        );
        assert!(
            text_content.contains("helper"),
            "Path variant '{}' should find helper function",
            path_variant
        );
    }

    Ok(())
}
