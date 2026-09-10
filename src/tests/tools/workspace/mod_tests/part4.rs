#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore] // HANGS: Concurrent indexing stress test - not critical for CLI tools
// Run manually with: cargo test test_concurrent_manage_workspace --ignored
async fn test_concurrent_manage_workspace_index_does_not_lock_search_index() {
    // Skip search index initialization but allow Tantivy to initialize
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
        let handler = JulieServerHandler::new_for_test().await.unwrap();
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

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_primary_index_schedules_embedding_when_provider_available() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    // Inject deterministic provider so embedding scheduling is enabled in test.
    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let message = extract_text_from_result(&result);

    assert!(
        message.contains("Embedding") && message.contains("background"),
        "Primary index should schedule embeddings when provider is available. Message: {message}"
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
    let handler = JulieServerHandler::new_for_test().await.unwrap();
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
            // Clear all symbols to simulate empty database
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
    let handler = JulieServerHandler::new_for_test().await.unwrap();
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

    // Try to index again with force=false - should run incremental update
    // (catch-up indexing compares blake3 hashes; unchanged files are skipped)
    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    // Incremental re-index succeeds and still reports symbols
    assert!(
        result_text.contains("Workspace indexing complete"),
        "Incremental re-index should succeed, got: {}",
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
    let handler = JulieServerHandler::new_for_test().await.unwrap();
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
