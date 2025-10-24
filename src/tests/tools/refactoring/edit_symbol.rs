//! Unit tests for EditSymbolTool
//!
//! Tests verify file-specific semantic editing with flat parameters

use crate::handler::JulieServerHandler;
use crate::tools::refactoring::{EditOperation, EditSymbolTool};
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

// ===== REPLACE BODY TESTS =====

#[tokio::test]
async fn test_edit_symbol_replace_body_basic() -> Result<()> {
    // Setup: Create temp workspace with a function
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn calculate() {\n    let x = 1 + 1;\n    x\n}")?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
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

    // NEW API: Flat parameters with enum for operation
    let tool = EditSymbolTool {
        file_path: test_file.to_string_lossy().to_string(),
        symbol_name: "calculate".to_string(),
        operation: EditOperation::ReplaceBody,
        content: "return 2 + 2;".to_string(),
        position: None,    // Not used for ReplaceBody
        target_file: None, // Not used for ReplaceBody
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;

    // Verify body replaced
    let content = fs::read_to_string(&test_file)?;
    assert!(content.contains("2 + 2"), "New body should be present");
    assert!(!content.contains("1 + 1"), "Old body should be gone");

    // Verify result indicates success
    let result_text = format!("{:?}", result);
    assert!(result_text.contains("Success") || result_text.contains("✅"));

    Ok(())
}

#[tokio::test]
async fn test_edit_symbol_replace_body_validation_no_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
        .await?;

    // ERROR CASE: File doesn't exist
    let tool = EditSymbolTool {
        file_path: "/nonexistent/file.rs".to_string(),
        symbol_name: "test".to_string(),
        operation: EditOperation::ReplaceBody,
        content: "new body".to_string(),
        position: None,
        target_file: None,
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await;
    assert!(
        result.is_err() || format!("{:?}", result).contains("not exist"),
        "Should reject non-existent file"
    );

    Ok(())
}

// ===== INSERT TESTS =====

#[tokio::test]
async fn test_edit_symbol_insert_after() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn first() {}\nfn second() {}")?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
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

    let tool = EditSymbolTool {
        file_path: test_file.to_string_lossy().to_string(),
        symbol_name: "first".to_string(),
        operation: EditOperation::InsertRelative,
        content: "fn inserted() {}".to_string(),
        position: Some("after".to_string()),
        target_file: None,
        dry_run: false,
    };

    let _result = tool.call_tool(&handler).await?;

    // Verify insertion
    let content = fs::read_to_string(&test_file)?;
    assert!(
        content.contains("inserted"),
        "New function should be inserted"
    );

    // Verify order: first, then inserted, then second
    let first_pos = content.find("first").unwrap();
    let inserted_pos = content.find("inserted").unwrap();
    let second_pos = content.find("second").unwrap();

    assert!(first_pos < inserted_pos, "Inserted should come after first");
    assert!(
        inserted_pos < second_pos,
        "Inserted should come before second"
    );

    Ok(())
}

#[tokio::test]
async fn test_edit_symbol_insert_before() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn first() {}\nfn second() {}")?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
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

    let tool = EditSymbolTool {
        file_path: test_file.to_string_lossy().to_string(),
        symbol_name: "second".to_string(),
        operation: EditOperation::InsertRelative,
        content: "fn inserted() {}".to_string(),
        position: Some("before".to_string()),
        target_file: None,
        dry_run: false,
    };

    let _result = tool.call_tool(&handler).await?;

    // Verify insertion order
    let content = fs::read_to_string(&test_file)?;
    let first_pos = content.find("first").unwrap();
    let inserted_pos = content.find("inserted").unwrap();
    let second_pos = content.find("second").unwrap();

    assert!(first_pos < inserted_pos, "Inserted should come after first");
    assert!(
        inserted_pos < second_pos,
        "Inserted should come before second"
    );

    Ok(())
}

// ===== EXTRACT TESTS =====

#[tokio::test]
async fn test_edit_symbol_extract_to_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("main.rs");
    let target_file = temp_dir.path().join("helper.rs");

    fs::write(
        &source_file,
        "fn helper() { return 42; }\nfn main() { helper(); }",
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
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

    let tool = EditSymbolTool {
        file_path: source_file.to_string_lossy().to_string(),
        symbol_name: "helper".to_string(),
        operation: EditOperation::ExtractToFile,
        content: String::new(), // Not used for extraction
        position: None,
        target_file: Some(target_file.to_string_lossy().to_string()),
        dry_run: false,
    };

    let _result = tool.call_tool(&handler).await?;

    // Verify extraction
    let source_content = fs::read_to_string(&source_file)?;
    let target_content = fs::read_to_string(&target_file)?;

    assert!(
        !source_content.contains("fn helper"),
        "Helper should be removed from source"
    );
    assert!(
        target_content.contains("fn helper"),
        "Helper should be in target"
    );
    assert!(
        target_content.contains("return 42"),
        "Helper body should be in target"
    );

    Ok(())
}

#[tokio::test]
async fn test_edit_symbol_extract_validation_no_target() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("main.rs");
    fs::write(&source_file, "fn test() {}")?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
        .await?;

    // ERROR CASE: ExtractToFile without target_file
    let tool = EditSymbolTool {
        file_path: source_file.to_string_lossy().to_string(),
        symbol_name: "test".to_string(),
        operation: EditOperation::ExtractToFile,
        content: String::new(),
        position: None,
        target_file: None, // Missing target_file!
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await;
    assert!(result.is_ok(), "Should return Ok with error message");
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("target_file") || result_text.contains("required"),
        "Should reject missing target_file: {}",
        result_text
    );

    Ok(())
}

// ===== DRY RUN TESTS =====

#[tokio::test]
async fn test_edit_symbol_dry_run() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn calculate() { 1 + 1 }")?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
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

    let tool = EditSymbolTool {
        file_path: test_file.to_string_lossy().to_string(),
        symbol_name: "calculate".to_string(),
        operation: EditOperation::ReplaceBody,
        content: "2 + 2".to_string(),
        position: None,
        target_file: None,
        dry_run: true, // DRY RUN
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    // Should show preview
    let result_text_lower = result_text.to_lowercase();
    assert!(
        result_text_lower.contains("preview")
            || result_text_lower.contains("would")
            || result_text_lower.contains("dry"),
        "Expected preview indicator: {}",
        result_text
    );

    // File should NOT be modified
    let content = fs::read_to_string(&test_file)?;
    assert!(content.contains("1 + 1"), "Dry run should not modify file");
    assert!(!content.contains("2 + 2"), "Dry run should not modify file");

    Ok(())
}
