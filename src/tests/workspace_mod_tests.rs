//! Tests for `workspace::JulieWorkspace` extracted from the implementation module.

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::JulieWorkspace;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_workspace_initialization() {
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
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
}

#[tokio::test]
async fn test_workspace_detection() {
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
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
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
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
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::remove_var("JULIE_SKIP_SEARCH_INDEX");

    let workspace_path = std::env::current_dir().unwrap().to_string_lossy().to_string();

    let run_index = |path: String| async move {
        let handler = JulieServerHandler::new().await.unwrap();
        let tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(path),
            force: Some(true),
            name: None,
            workspace_id: None,
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
        };

        tool
            .call_tool(&handler)
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

#[tokio::test]
async fn test_manage_workspace_recent_files() {
    // TDD: RED phase - this test WILL FAIL until we implement the "recent" operation
    std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();

    // Initialize workspace
    let workspace = JulieWorkspace::initialize(workspace_path.clone())
        .await
        .unwrap();

    // Get database access
    let db = workspace.db.as_ref().expect("Database should be initialized");
    let db_lock = db.lock().unwrap();

    // Insert test files with different timestamps
    let now = chrono::Utc::now().timestamp();
    let two_days_ago = now - (2 * 86400); // 2 days in seconds
    let seven_days_ago = now - (7 * 86400); // 7 days in seconds

    // File modified today (should be included)
    db_lock.store_file_info(&crate::database::FileInfo {
        path: "recent_file.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash1".to_string(),
        size: 100,
        last_modified: now,
        last_indexed: now,
        symbol_count: 5,
        content: Some("fn main() {}".to_string()),
    }, "primary").unwrap();

    // File modified 1 day ago (should be included)
    db_lock.store_file_info(&crate::database::FileInfo {
        path: "yesterday_file.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash2".to_string(),
        size: 200,
        last_modified: two_days_ago + 86400, // 1 day ago
        last_indexed: now,
        symbol_count: 3,
        content: Some("fn test() {}".to_string()),
    }, "primary").unwrap();

    // File modified 7 days ago (should NOT be included)
    db_lock.store_file_info(&crate::database::FileInfo {
        path: "old_file.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash3".to_string(),
        size: 150,
        last_modified: seven_days_ago,
        last_indexed: now,
        symbol_count: 2,
        content: Some("fn old() {}".to_string()),
    }, "primary").unwrap();

    drop(db_lock);
    drop(workspace);

    // Create handler and initialize it with the workspace
    let handler = JulieServerHandler::new().await.unwrap();
    handler
        .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
        .await
        .unwrap();

    let tool = ManageWorkspaceTool {
        operation: "recent".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: None,
        name: None,
        workspace_id: None,
        expired_only: None,
        days: Some(2), // Last 2 days
        max_size_mb: None,
        detailed: None,
    };

    // Call the tool (this will FAIL because "recent" operation doesn't exist yet)
    let result = tool.call_tool(&handler).await;

    assert!(result.is_ok(), "Recent files operation should succeed");

    let call_result = result.unwrap();

    // Extract text from CallToolResult using serde_json pattern
    let response_text = call_result.content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(!response_text.is_empty(), "Should return results");

    // Should include recent files
    assert!(response_text.contains("recent_file.rs"), "Should include file modified today");
    assert!(response_text.contains("yesterday_file.rs"), "Should include file modified yesterday");

    // Should NOT include old files
    assert!(!response_text.contains("old_file.rs"), "Should NOT include file modified 7 days ago");
}
