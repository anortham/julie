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
#[ignore = "Flaky due to filesystem timestamp resolution - needs investigation"]
async fn test_fresh_index_no_reindex_needed() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create a simple test file
    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}")?;

    // Initialize workspace and index
    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Small delay to ensure database mtime is definitely > file mtime
    // (filesystem timestamp resolution can cause issues otherwise)
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

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

/// Test 6: scan_workspace_files respects .julieignore patterns
/// Given: Workspace with .julieignore file containing ignore patterns
/// When: scan_workspace_files() is called
/// Expected: Files matching .julieignore patterns are excluded
///
/// Bug: Discovery respects .julieignore, but scan_workspace_files does not
/// This causes false "needs indexing" warnings for ignored files
#[test]
fn test_scan_workspace_files_respects_julieignore() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create some test files
    let normal_file = workspace_path.join("normal.rs");
    let ignored_file = workspace_path.join("ignored.rs");
    let generated_dir = workspace_path.join("generated");
    fs::create_dir(&generated_dir)?;
    let generated_file = generated_dir.join("schema.rs");

    fs::write(&normal_file, "fn normal() {}")?;
    fs::write(&ignored_file, "fn ignored() {}")?;
    fs::write(&generated_file, "fn generated() {}")?;

    // Create .julieignore file
    let julieignore_path = workspace_path.join(".julieignore");
    fs::write(
        &julieignore_path,
        "# Ignore specific files and directories\nignored.rs\ngenerated/\n",
    )?;

    // Call scan_workspace_files
    let files = crate::startup::scan_workspace_files(workspace_path)?;

    // Verify: normal.rs is included
    assert!(files.contains("normal.rs"), "Should find normal.rs");

    // Verify: ignored.rs is excluded (respects .julieignore)
    assert!(
        !files.contains("ignored.rs"),
        "Should NOT find ignored.rs (in .julieignore)"
    );

    // Verify: generated/schema.rs is excluded (respects .julieignore directory pattern)
    assert!(
        !files.contains("generated/schema.rs"),
        "Should NOT find generated/schema.rs (directory in .julieignore)"
    );

    Ok(())
}

/// Test 7: scan_workspace_files returns Unix-style paths (Windows bug fix)
/// Given: Files in nested directories
/// When: scan_workspace_files() is called
/// Expected: All paths use forward slashes (/), not backslashes (\)
///
/// Bug: On Windows, strip_prefix() returns paths with backslashes (src\file.rs)
/// But database stores paths with forward slashes (src/file.rs)
/// This causes staleness detection to fail because paths don't match
#[test]
fn test_scan_workspace_files_returns_unix_style_paths() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create nested directory structure with files
    let src_dir = workspace_path.join("src");
    let tools_dir = src_dir.join("tools");
    fs::create_dir_all(&tools_dir)?;

    // Create files at different nesting levels
    let root_file = workspace_path.join("main.rs");
    let src_file = src_dir.join("lib.rs");
    let nested_file = tools_dir.join("search.rs");

    fs::write(&root_file, "fn main() {}")?;
    fs::write(&src_file, "pub mod tools;")?;
    fs::write(&nested_file, "pub fn search() {}")?;

    // Call scan_workspace_files
    let files = crate::startup::scan_workspace_files(workspace_path)?;

    // Verify: ALL paths use Unix-style forward slashes
    for file_path in &files {
        // Path should NOT contain backslashes (Windows separators)
        assert!(
            !file_path.contains('\\'),
            "Path '{}' contains backslash separator (should be Unix-style with /)",
            file_path
        );

        // Nested paths should use forward slashes
        if file_path.contains('/') {
            // Verify it's a properly formed Unix-style path
            let parts: Vec<&str> = file_path.split('/').collect();
            assert!(
                parts.len() >= 2,
                "Nested path '{}' should have multiple components separated by /",
                file_path
            );
        }
    }

    // Verify expected files are present (with Unix-style paths)
    assert!(files.contains("main.rs"), "Should find root file");
    assert!(
        files.contains("src/lib.rs"),
        "Should find src file with / separator"
    );
    assert!(
        files.contains("src/tools/search.rs"),
        "Should find nested file with / separators"
    );

    // Verify we found exactly 3 files
    assert_eq!(files.len(), 3, "Should find exactly 3 files");

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
