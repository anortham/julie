//! Unit tests for RenameSymbolTool
//!
//! Tests verify workspace-wide symbol renaming with flat parameters

use crate::handler::JulieServerHandler;
use crate::tools::refactoring::RenameSymbolTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_rename_symbol_basic() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    // Setup: Create temp workspace with a Rust file
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn getUserData() { println!(\"test\"); }")?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // Index workspace for symbol lookup
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // NEW API: Flat parameters (no JSON string)
    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "fetchUserData".to_string(),
        scope: None,
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Debug: Print result
    eprintln!("Rename result: {:?}", result);

    // Verify rename occurred
    let content = fs::read_to_string(&test_file)?;
    eprintln!("File content after rename: {}", content);
    assert!(
        content.contains("fetchUserData"),
        "Symbol should be renamed, but file contains: {}",
        content
    );
    assert!(
        !content.contains("getUserData"),
        "Old symbol should be gone"
    );

    // Verify result text confirms the rename
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("applied") && result_text.contains("change"),
        "Result should confirm applied changes, got: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_validation_same_name() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // ERROR CASE: old_name == new_name
    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "getUserData".to_string(), // Same name!
        scope: None,
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await;

    // Should return Ok with error message
    assert!(result.is_ok(), "Should return Ok with error message");
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("same") || result_text.contains("identical"),
        "Should reject same old/new names: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_validation_empty_names() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // ERROR CASE: Empty old_name
    let tool = RenameSymbolTool {
        old_name: "".to_string(), // Empty!
        new_name: "fetchUserData".to_string(),
        scope: None,
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await;
    assert!(result.is_ok(), "Should return Ok with error message");
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("empty") || result_text.contains("required"),
        "Should reject empty old_name: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_dry_run() -> Result<()> {
    // Dry run should preview changes without modifying files
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn getUserData() {}")?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // Index workspace for symbol lookup
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "fetchUserData".to_string(),
        scope: None,
        dry_run: true, // DRY RUN
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    // Should show preview
    let result_text_lower = result_text.to_lowercase();
    assert!(
        result_text_lower.contains("preview")
            || result_text_lower.contains("would")
            || result_text_lower.contains("dry"),
        "Expected preview indicator in result: {}",
        result_text
    );

    // File should NOT be modified
    let content = fs::read_to_string(&test_file)?;
    assert!(
        content.contains("getUserData"),
        "Dry run should not modify file"
    );
    assert!(
        !content.contains("fetchUserData"),
        "Dry run should not modify file"
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_multiple_files() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    // Verify workspace-wide rename across multiple files
    let temp_dir = TempDir::new()?;
    let file1 = temp_dir.path().join("main.rs");
    let file2 = temp_dir.path().join("lib.rs");

    fs::write(&file1, "fn getUserData() { /* main */ }")?;
    fs::write(
        &file2,
        "use crate::getUserData; fn test() { getUserData(); }",
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // Index workspace for symbol lookup
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "fetchUserData".to_string(),
        scope: None,
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Verify both files modified
    let content1 = fs::read_to_string(&file1)?;
    let content2 = fs::read_to_string(&file2)?;

    assert!(
        content1.contains("fetchUserData"),
        "File 1 should be renamed"
    );
    assert!(
        content2.contains("fetchUserData"),
        "File 2 should be renamed"
    );

    // Verify result reports multiple files
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("2") || result_text.contains("files"),
        "Should report multiple files changed: {}",
        result_text
    );

    Ok(())
}
