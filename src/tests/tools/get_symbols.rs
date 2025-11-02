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
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Explicitly trigger indexing (initialize_workspace doesn't auto-index)
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
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
    let db_lock = db.lock().unwrap();
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
        target: None,
        limit: None,
        mode: None,
        workspace: None,
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
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Explicitly trigger indexing
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

    // Query using ABSOLUTE path (should work even before fix)
    let absolute_path = test_file.to_string_lossy().to_string();
    let tool = GetSymbolsTool {
        file_path: absolute_path,
        max_depth: 1,
        target: None,
        limit: None,
        mode: None,
        workspace: None,
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
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Explicitly trigger indexing
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
            target: None,
            limit: None,
            mode: None,
            workspace: None,
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

#[tokio::test]
async fn test_get_symbols_with_limit_parameter() -> Result<()> {
    // Test that the limit parameter truncates results correctly

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create a file with many symbols (20 functions)
    let test_file = src_dir.join("many_symbols.rs");
    let mut content = String::new();
    for i in 1..=20 {
        content.push_str(&format!("pub fn function_{}() -> i32 {{ {} }}\n\n", i, i));
    }
    fs::write(&test_file, content)?;

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

    // Test 1: No limit - should return all 20 symbols
    let tool_no_limit = GetSymbolsTool {
        file_path: "src/many_symbols.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: None,
        workspace: None,
    };

    let result_no_limit = tool_no_limit.call_tool(&handler).await?;
    let structured_content_no_limit = result_no_limit
        .structured_content
        .expect("Should have structured content");

    let total_symbols_no_limit = structured_content_no_limit
        .get("total_symbols")
        .and_then(|v| v.as_u64())
        .expect("Should have total_symbols");

    assert_eq!(total_symbols_no_limit, 20, "Should find all 20 symbols");

    // Test 2: With limit=5 - should return only 5 symbols
    let tool_with_limit = GetSymbolsTool {
        file_path: "src/many_symbols.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: Some(5),
        mode: None,
        workspace: None,
    };

    let result_with_limit = tool_with_limit.call_tool(&handler).await?;
    let text_with_limit = extract_text_from_result(&result_with_limit);
    let structured_content_with_limit = result_with_limit
        .structured_content
        .expect("Should have structured content");

    let returned_symbols = structured_content_with_limit
        .get("returned_symbols")
        .and_then(|v| v.as_u64())
        .expect("Should have returned_symbols");

    let total_symbols = structured_content_with_limit
        .get("total_symbols")
        .and_then(|v| v.as_u64())
        .expect("Should have total_symbols");

    let truncated = structured_content_with_limit
        .get("truncated")
        .and_then(|v| v.as_bool())
        .expect("Should have truncated flag");

    assert_eq!(returned_symbols, 5, "Should return exactly 5 symbols");
    assert_eq!(total_symbols, 20, "Total should still be 20");
    assert!(truncated, "Should indicate truncation occurred");
    assert!(
        text_with_limit.contains("⚠️"),
        "Text should show truncation warning"
    );
    assert!(
        text_with_limit.contains("Showing 5 of 20"),
        "Should show truncation summary: {}",
        text_with_limit
    );

    Ok(())
}

#[tokio::test]
async fn test_get_symbols_file_not_found_error() -> Result<()> {
    // Test that we get a clear "File not found" error vs "No symbols found"

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create ONE file that exists
    let existing_file = src_dir.join("exists.rs");
    fs::write(&existing_file, "pub fn test() -> i32 { 42 }")?;

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

    // Test 1: Query non-existent file - should get "File not found"
    let tool_not_found = GetSymbolsTool {
        file_path: "src/does_not_exist.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: None,
        workspace: None,
    };

    let result_not_found = tool_not_found.call_tool(&handler).await?;
    let text_not_found = extract_text_from_result(&result_not_found);

    // Should explicitly say "File not found"
    assert!(
        text_not_found.contains("File not found"),
        "Should say 'File not found' for non-existent files, got: {}",
        text_not_found
    );
    assert!(
        text_not_found.contains("❌"),
        "Should include error emoji for visibility"
    );

    // Test 2: Query file that exists WITH symbols - should work
    let tool_exists = GetSymbolsTool {
        file_path: "src/exists.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: None,
        workspace: None,
    };

    let result_exists = tool_exists.call_tool(&handler).await?;
    let text_exists = extract_text_from_result(&result_exists);

    assert!(
        !text_exists.contains("File not found"),
        "Existing file should not trigger 'File not found'"
    );
    assert!(
        !text_exists.contains("No symbols found"),
        "Should find symbols in existing file"
    );
    assert!(
        text_exists.contains("test"),
        "Should find test function in symbols"
    );

    // Test 3: Create empty file (exists but has no code) - should get "No symbols found"
    let empty_file = src_dir.join("empty.rs");
    fs::write(&empty_file, "")?;

    // Re-index to pick up new empty file
    index_tool.call_tool(&handler).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let tool_empty = GetSymbolsTool {
        file_path: "src/empty.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: None,
        workspace: None,
    };

    let result_empty = tool_empty.call_tool(&handler).await?;
    let text_empty = extract_text_from_result(&result_empty);

    // Empty file EXISTS, so should NOT say "File not found"
    assert!(
        !text_empty.contains("File not found"),
        "Empty file exists, should not say 'File not found'"
    );
    // Empty file has no symbols, so SHOULD say "No symbols found"
    assert!(
        text_empty.contains("No symbols found"),
        "Empty file should say 'No symbols found', got: {}",
        text_empty
    );

    Ok(())
}
