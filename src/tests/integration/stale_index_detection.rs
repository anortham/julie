//! TDD Tests for Stale Index Detection
//!
//! These tests verify that Julie correctly detects when the index is stale and needs re-indexing.
//!
//! Test Scenarios:
//! 1. Fresh index (no indexing needed)
//! 2. Stale index - file modified after last index
//! 3. New files added that aren't in the database
//! 4. Database completely empty (existing behavior)

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

/// Test 1: Fresh index - no indexing needed
/// Given: Database is up-to-date with all files
/// When: check_if_indexing_needed() is called
/// Expected: Returns false (no indexing needed)
#[tokio::test]
async fn test_fresh_index_no_reindex_needed() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create a simple test file
    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}")?;

    // Initialize workspace and index
    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Verify: No indexing needed (database is fresh)
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(!needs_indexing, "Fresh index should not need re-indexing");

    Ok(())
}

/// Test 2: Stale index - file modified after last index
/// Given: File is modified AFTER database was last updated
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (indexing needed)
#[tokio::test]
async fn test_stale_index_file_modified_after_db() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create and index a test file
    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Sleep to ensure mtime changes
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Modify the file AFTER indexing
    fs::write(&test_file, "fn hello() { println!(\"world\"); }")?;

    // Verify: Indexing IS needed (file is newer than database)
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(needs_indexing, "Modified file should trigger re-indexing");

    Ok(())
}

/// Test 3: New file added that isn't in database
/// Given: A new file exists that wasn't indexed
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (indexing needed)
#[tokio::test]
async fn test_new_file_not_in_database() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create and index first file
    let first_file = workspace_path.join("first.rs");
    fs::write(&first_file, "fn first() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Add a NEW file that isn't indexed
    let new_file = workspace_path.join("second.rs");
    fs::write(&new_file, "fn second() {}")?;

    // Verify: Indexing IS needed (new file detected)
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(needs_indexing, "New file should trigger re-indexing");

    Ok(())
}

/// Test 4: Empty database still triggers indexing (existing behavior)
/// Given: Database is completely empty
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (indexing needed)
#[tokio::test]
async fn test_empty_database_needs_indexing() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create test file but DON'T index
    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}")?;

    let handler = create_test_handler(workspace_path).await?;

    // Verify: Indexing IS needed (database is empty)
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(needs_indexing, "Empty database should need indexing");

    Ok(())
}

/// Test 5: Multiple stale files trigger re-indexing
/// Given: Multiple files modified after last index
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (indexing needed)
#[tokio::test]
async fn test_multiple_stale_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create and index multiple files
    let file1 = workspace_path.join("file1.rs");
    let file2 = workspace_path.join("file2.rs");
    fs::write(&file1, "fn one() {}")?;
    fs::write(&file2, "fn two() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Sleep to ensure mtime changes
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Modify BOTH files after indexing
    fs::write(&file1, "fn one() { println!(\"modified\"); }")?;
    fs::write(&file2, "fn two() { println!(\"modified\"); }")?;

    // Verify: Indexing IS needed
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(
        needs_indexing,
        "Multiple modified files should trigger re-indexing"
    );

    Ok(())
}

// ============================================================================
// Test Helpers
// ============================================================================

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;

async fn create_test_handler(workspace_path: &std::path::Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;
    Ok(handler)
}

async fn index_workspace(
    handler: &JulieServerHandler,
    workspace_path: &std::path::Path,
) -> Result<()> {
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    index_tool.call_tool(handler).await?;
    Ok(())
}
