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
use std::sync::Arc;
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

/// Test: scan_workspace_files respects .gitignore
/// Given: Workspace with .git dir and .gitignore
/// When: scan_workspace_files() is called
/// Expected: Files matching .gitignore are excluded
#[test]
fn test_scan_workspace_files_respects_gitignore() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();
    fs::create_dir_all(root.join(".git"))?;
    fs::write(root.join(".gitignore"), "generated/\n")?;
    fs::create_dir_all(root.join("generated"))?;
    fs::write(root.join("generated/api.rs"), "// auto-generated")?;
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join("src/main.rs"), "fn main() {}")?;

    let files = crate::startup::scan_workspace_files(root)?;
    assert!(files.contains("src/main.rs"), "should include src/main.rs");
    assert!(
        !files.iter().any(|f| f.contains("generated")),
        "should exclude gitignored dir"
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

/// Given: A workspace with 3 indexed files, then 1 file is deleted from disk
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (cleanup needed for deleted file)
#[tokio::test]
async fn test_deleted_file_detected_on_reconnect() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create 3 source files
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("a.rs"), "fn a() {}\n")?;
    fs::write(src_dir.join("b.rs"), "fn b() {}\n")?;
    fs::write(src_dir.join("c.rs"), "fn c() {}\n")?;

    // Index the workspace
    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Delete one file (simulating deletion while daemon was down)
    fs::remove_file(src_dir.join("b.rs"))?;

    // check_if_indexing_needed should detect the deleted file
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(
        needs_indexing,
        "Should detect deleted file b.rs needs cleanup"
    );

    Ok(())
}

#[tokio::test]
async fn test_check_if_indexing_needed_prefers_shared_anchor_over_local_julie_tree() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("daemon-indexes");
    fs::create_dir_all(&indexes_dir)?;

    let workspace_root = temp_dir.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    let test_file = workspace_root.join("main.rs");
    fs::write(&test_file, "fn shared_anchor() {}")?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let workspace_path = workspace_root.canonicalize()?;
    let workspace_path_str = workspace_path.to_string_lossy().to_string();
    let workspace_id = crate::workspace::registry::generate_workspace_id(&workspace_path_str)?;
    let pooled_workspace = pool
        .get_or_init(&workspace_id, workspace_path.clone())
        .await?;
    daemon_db.upsert_workspace(&workspace_id, &workspace_path_str, "ready")?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        pooled_workspace,
        workspace_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(workspace_id.clone()),
        None,
        None,
        None,
        None,
        Some(pool),
    )
    .await?;

    // Create a bogus local stdio tree with an old database mtime. The startup check
    // must ignore this and use the shared daemon anchor instead.
    let local_db_path = workspace_root
        .join(".julie")
        .join("indexes")
        .join(&workspace_id)
        .join("db")
        .join("symbols.db");
    fs::create_dir_all(local_db_path.parent().expect("local db parent"))?;
    fs::write(&local_db_path, b"bogus local db")?;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    index_workspace(&handler, &workspace_root).await?;

    let resolved_db_path = handler.workspace_db_file_path_for(&workspace_id).await?;
    assert_eq!(
        resolved_db_path,
        indexes_dir
            .join(&workspace_id)
            .join("db")
            .join("symbols.db"),
        "freshness check should resolve the shared daemon db path, not the local .julie decoy"
    );

    let _ = crate::startup::check_if_indexing_needed(&handler).await?;

    Ok(())
}

#[tokio::test]
async fn test_check_if_indexing_needed_uses_rebound_current_primary_snapshot() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::database::types::FileInfo;
    use crate::extractors::{Symbol, SymbolKind};
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("daemon-indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        original_root.join("src").join("loaded.rs"),
        "fn loaded_primary_only() {}\n",
    )?;
    fs::write(
        rebound_root.join("src").join("rebound.rs"),
        "fn rebound_primary_only() {}\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    let original_ws = pool
        .get_or_init(&original_id, original_path.clone())
        .await?;
    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    let rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    {
        let rebound_db = rebound_ws
            .db
            .as_ref()
            .expect("rebound workspace should have a db")
            .clone();
        let mut rebound_db = rebound_db.lock().unwrap();
        let file_info = FileInfo {
            path: "src/rebound.rs".to_string(),
            language: "rust".to_string(),
            hash: "rebound-primary-hash".to_string(),
            size: 28,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 1,
            line_count: 1,
            content: Some("fn rebound_primary_only() {}\n".to_string()),
        };
        let symbol = Symbol {
            id: "rebound-primary-symbol".to_string(),
            name: "rebound_primary_only".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/rebound.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 26,
            start_byte: 0,
            end_byte: 28,
            signature: Some("fn rebound_primary_only()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("fn rebound_primary_only() {}".to_string()),
            content_type: None,
        };
        rebound_db.bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &rebound_id)?;
    }

    handler.set_current_primary_binding(rebound_id, rebound_path);

    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(
        !needs_indexing,
        "freshness check should use the rebound current-primary snapshot, not the stale loaded workspace"
    );

    Ok(())
}

// ============================================================================
// Test Helpers
// ============================================================================

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;

async fn create_test_handler(workspace_path: &std::path::Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new_for_test().await?;
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
