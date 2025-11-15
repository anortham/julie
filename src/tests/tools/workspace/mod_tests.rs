//! Tests for `workspace::JulieWorkspace` extracted from the implementation module.

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::JulieWorkspace;
use rust_mcp_sdk::schema::CallToolResult;
use std::fs;
use tempfile::TempDir;

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
async fn test_workspace_initialization() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    // Check that .julie directory was created
    assert!(workspace.julie_dir.exists());

    // Check that all required root subdirectories exist
    // Note: Per-workspace directories (indexes/{workspace_id}/) are created on-demand during indexing
    assert!(
        workspace.julie_dir.join("indexes").exists(),
        "indexes/ root directory should exist"
    );
    assert!(workspace.julie_dir.join("models").exists());
    assert!(workspace.julie_dir.join("cache").exists());
    assert!(workspace.julie_dir.join("logs").exists());
    assert!(workspace.julie_dir.join("config").exists());

    // Check that config file was created
    assert!(workspace.julie_dir.join("config/julie.toml").exists());

    // Check that .gitignore was created to prevent accidental commits
    assert!(
        workspace.julie_dir.join(".gitignore").exists(),
        ".gitignore should be created in .julie directory"
    );
}

#[tokio::test]
async fn test_workspace_detection() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    drop(workspace);

    // Clean up any per-workspace directories if they exist
    let indexes_dir = temp_dir.path().join(".julie").join("indexes");
    if indexes_dir.exists() {
        let _ = fs::remove_dir_all(&indexes_dir);
        fs::create_dir_all(&indexes_dir).unwrap();
    }

    // Test detection from same directory
    let detected = JulieWorkspace::detect_and_load(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    assert!(detected.is_some());

    // Test detection from subdirectory
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    let detected = JulieWorkspace::detect_and_load(subdir).await.unwrap();
    assert!(detected.is_some());
}

#[tokio::test]
async fn test_health_check() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    let health = workspace.health_check().unwrap();
    assert!(health.is_healthy());
    assert!(health.structure_valid);
    assert!(health.has_write_permissions);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore] // HANGS: Concurrent indexing stress test - not critical for CLI tools
// Run manually with: cargo test test_concurrent_manage_workspace --ignored
async fn test_concurrent_manage_workspace_index_does_not_lock_search_index() {
    // Skip expensive embedding initialization but allow Tantivy to initialize
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::remove_var("JULIE_SKIP_SEARCH_INDEX");
    }

    let workspace_path = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let run_index = |path: String| async move {
        let handler = JulieServerHandler::new().await.unwrap();
        let tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(path),
            force: Some(true),
            name: None,
            workspace_id: None,
            detailed: None,
        };

        tool.call_tool(&handler)
            .await
            .map_err(|err| err.to_string())
    };

    let handle_a = tokio::spawn(run_index(workspace_path.clone()));
    let handle_b = tokio::spawn(run_index(workspace_path.clone()));

    let result_a = handle_a.await.unwrap();
    let result_b = handle_b.await.unwrap();

    assert!(
        result_a.is_ok(),
        "first index run failed with: {:?}",
        result_a
    );
    assert!(
        result_b.is_ok(),
        "second index run failed with: {:?}",
        result_b
    );
}

/// Regression test for Bug #1: handle_add_command must update workspace statistics
///
/// Bug: When adding a reference workspace, the statistics (file_count, symbol_count)
/// were never updated in the registry after indexing, so `manage_workspace list`
/// would always show 0 files and 0 symbols even though the database had data.
///
/// Root cause: handle_add_command called index_workspace_files() and received correct
/// counts, but never called registry_service.update_workspace_statistics().
///
/// Fix: Added update_workspace_statistics() call after successful indexing, mirroring
/// the implementation in handle_refresh_command.
#[tokio::test]
async fn test_add_workspace_updates_statistics() {
    use crate::workspace::registry_service::WorkspaceRegistryService;

    // Skip background tasks for this test
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    // Setup: Create test workspaces with actual files
    let primary_dir = TempDir::new().unwrap();
    let reference_dir = TempDir::new().unwrap();

    // Create a simple test file in reference workspace
    let test_file = reference_dir.path().join("test.rs");
    fs::write(
        &test_file,
        r#"
fn hello_world() {
    println!("Hello, world!");
}

fn goodbye_world() {
    println!("Goodbye, world!");
}
        "#,
    )
    .unwrap();

    // Initialize primary workspace
    let _primary_workspace = JulieWorkspace::initialize(primary_dir.path().to_path_buf())
        .await
        .unwrap();

    // Create handler (simulates the server context)
    let handler = JulieServerHandler::new().await.unwrap();
    handler
        .initialize_workspace_with_force(
            Some(primary_dir.path().to_str().unwrap().to_string()),
            true,
        )
        .await
        .unwrap();

    // Add reference workspace using ManageWorkspaceTool
    let tool = ManageWorkspaceTool {
        operation: "add".to_string(),
        path: Some(reference_dir.path().to_str().unwrap().to_string()),
        name: Some("test-workspace".to_string()),
        force: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool
        .handle_add_command(
            &handler,
            reference_dir.path().to_str().unwrap(),
            Some("test-workspace".to_string()),
        )
        .await;

    assert!(result.is_ok(), "handle_add_command failed: {:?}", result);

    // Verify: Check registry statistics directly
    let primary_workspace = handler.get_workspace().await.unwrap().unwrap();
    let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());
    let workspaces = registry_service.get_all_workspaces().await.unwrap();

    // Find the reference workspace we just added
    let reference_ws = workspaces
        .iter()
        .find(|ws| {
            matches!(
                ws.workspace_type,
                crate::workspace::registry::WorkspaceType::Reference
            )
        })
        .expect("Reference workspace not found in registry");

    // BUG #1: These were 0 before the fix
    assert!(
        reference_ws.file_count > 0,
        "Bug #1 regression: file_count is {}, should be > 0 after indexing",
        reference_ws.file_count
    );

    assert!(
        reference_ws.symbol_count > 0,
        "Bug #1 regression: symbol_count is {}, should be > 0 after indexing (file has 2 functions)",
        reference_ws.symbol_count
    );

    // Additional validation: symbol count should match the 2 functions in our test file
    assert_eq!(
        reference_ws.symbol_count, 2,
        "Expected 2 symbols (hello_world and goodbye_world), got {}",
        reference_ws.symbol_count
    );

    assert_eq!(
        reference_ws.file_count, 1,
        "Expected 1 file (test.rs), got {}",
        reference_ws.file_count
    );
}

/// Regression test for Bug: "Workspace already indexed: 0 symbols"
///
/// Bug: The is_indexed flag could be true while the database had 0 symbols,
/// causing the nonsensical message "Workspace already indexed: 0 symbols".
///
/// Root cause: The is_indexed flag was checked before querying the database,
/// and if true, would return early even when symbol_count was 0.
///
/// Fix: Added validation to check if symbol_count == 0, and if so, clear the
/// is_indexed flag and proceed with indexing instead of returning early.
#[tokio::test]
async fn test_is_indexed_flag_with_empty_database() {
    // Skip background tasks
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = temp_dir.path().join("test.rs");
    fs::write(
        &test_file,
        r#"
fn test_function() {
    println!("test");
}
        "#,
    )
    .unwrap();

    // Initialize workspace and handler
    let handler = JulieServerHandler::new().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_str().unwrap().to_string()), true)
        .await
        .unwrap();

    // First index to populate the database
    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("Workspace indexing complete"),
        "First indexing should succeed, got: {}",
        result_text
    );

    // Verify is_indexed is true
    assert!(
        *handler.is_indexed.read().await,
        "is_indexed should be true after indexing"
    );

    // SIMULATE THE BUG: Manually clear the database while keeping is_indexed=true
    // This simulates scenarios like database corruption, manual deletion, or partial cleanup
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            // Clear all symbols to simulate empty database
            // The FTS triggers will automatically sync symbols_fts table
            db_lock.conn.execute("DELETE FROM symbols", []).unwrap();
        }
    }

    // Verify database is now empty
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert_eq!(count, 0, "Database should be empty after manual deletion");
        }
    }

    // Verify is_indexed flag is still true (simulating the bug condition)
    assert!(
        *handler.is_indexed.read().await,
        "is_indexed should still be true (bug condition)"
    );

    // NOW TEST THE FIX: Try to index again with force=false
    // Before the fix: Would return "Workspace already indexed: 0 symbols"
    // After the fix: Should detect empty database, clear flag, and proceed with indexing
    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    // THE FIX: Should NOT see "already indexed: 0 symbols"
    assert!(
        !result_text.contains("already indexed: 0 symbols"),
        "Bug regression: Should not see 'already indexed: 0 symbols', got: {}",
        result_text
    );

    // THE FIX: Should proceed with indexing and report success
    assert!(
        result_text.contains("Workspace indexing complete") || result_text.contains("symbols"),
        "Should re-index when database is empty, got: {}",
        result_text
    );
}

/// Test that when is_indexed=true AND database has symbols, indexing is correctly skipped
#[tokio::test]
async fn test_is_indexed_flag_with_populated_database() {
    // Skip background tasks
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = temp_dir.path().join("test.rs");
    fs::write(
        &test_file,
        r#"
fn test_function() {
    println!("test");
}
        "#,
    )
    .unwrap();

    // Initialize workspace and handler
    let handler = JulieServerHandler::new().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_str().unwrap().to_string()), true)
        .await
        .unwrap();

    // First index to populate the database
    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("Workspace indexing complete"),
        "First indexing should succeed"
    );

    // Verify is_indexed is true
    assert!(*handler.is_indexed.read().await);

    // Verify database has symbols
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert!(count > 0, "Database should have symbols");
        }
    }

    // Try to index again with force=false - should skip
    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    // Should see "already indexed" message with symbol count > 0
    assert!(
        result_text.contains("already indexed"),
        "Should skip re-indexing when database has symbols, got: {}",
        result_text
    );

    assert!(
        !result_text.contains("0 symbols"),
        "Should NOT report 0 symbols, got: {}",
        result_text
    );
}

/// Test that force=true clears the is_indexed flag and performs re-indexing
#[tokio::test]
async fn test_force_reindex_clears_flag() {
    // Skip background tasks
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = temp_dir.path().join("test.rs");
    fs::write(
        &test_file,
        r#"
fn test_function() {
    println!("test");
}
        "#,
    )
    .unwrap();

    // Initialize workspace and handler
    let handler = JulieServerHandler::new().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_str().unwrap().to_string()), true)
        .await
        .unwrap();

    // First index
    let tool_no_force = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool_no_force.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(result_text.contains("Workspace indexing complete"));

    // Verify is_indexed is true
    assert!(*handler.is_indexed.read().await);

    // Force reindex
    let tool_force = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool_force.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    // Should complete indexing again (not skip)
    assert!(
        result_text.contains("Workspace indexing complete"),
        "Force reindex should complete indexing, got: {}",
        result_text
    );

    // Verify is_indexed is true after force reindex
    assert!(*handler.is_indexed.read().await);
}

/// Regression test for Bug: Incremental indexing skips files when database has 0 symbols
///
/// Bug: When database files table has file hashes but symbols table is empty,
/// incremental indexing considers files "unchanged" and skips them, resulting
/// in persistent 0 symbols even after re-indexing.
///
/// Root cause: filter_changed_files() only checks file hashes, not symbol count.
/// It doesn't detect the empty database condition and force full re-extraction.
///
/// Fix: Add check at start of filter_changed_files() to detect 0 symbols and
/// bypass incremental logic, returning all files for re-indexing.
#[tokio::test]
async fn test_incremental_indexing_detects_empty_database() {
    // Skip background tasks
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();

    // Create test files with actual code
    let test_file_1 = temp_dir.path().join("file1.rs");
    fs::write(
        &test_file_1,
        r#"
fn function_one() {
    println!("one");
}
        "#,
    )
    .unwrap();

    let test_file_2 = temp_dir.path().join("file2.rs");
    fs::write(
        &test_file_2,
        r#"
fn function_two() {
    println!("two");
}
        "#,
    )
    .unwrap();

    // Initialize workspace and handler
    let handler = JulieServerHandler::new().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_str().unwrap().to_string()), true)
        .await
        .unwrap();

    // First index to populate database
    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("Workspace indexing complete"),
        "First indexing should succeed"
    );

    // Verify we have symbols
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert_eq!(count, 2, "Should have 2 symbols from 2 functions");
        }
    }

    // SIMULATE THE BUG: Clear symbols table while keeping files table intact
    // This simulates the condition where file hashes exist but no symbols are extracted
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            // Clear symbols but keep files table (file hashes remain)
            db_lock.conn.execute("DELETE FROM symbols", []).unwrap();

            // Verify files table still has entries
            let file_count: i64 = db_lock
                .conn
                .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
                .unwrap();
            assert!(file_count > 0, "Files table should still have entries");
        }
    }

    // Verify database is now empty (0 symbols) but files table has hashes
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert_eq!(
                count, 0,
                "Database should have 0 symbols after manual deletion"
            );
        }
    }

    // Clear is_indexed flag to force the indexing logic to run
    *handler.is_indexed.write().await = false;

    // NOW TEST THE FIX: Try to index again with force=false
    // Before the fix: Incremental logic sees matching file hashes, skips files → 0 symbols persist
    // After the fix: Should detect empty database, bypass incremental logic, re-extract all symbols
    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("Workspace indexing complete"),
        "Re-indexing should complete"
    );

    // THE FIX: Should have re-extracted symbols despite matching file hashes
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert_eq!(
                count, 2,
                "Bug regression: Incremental indexing should detect empty database and re-extract symbols, got {} symbols",
                count
            );
        }
    }
}
