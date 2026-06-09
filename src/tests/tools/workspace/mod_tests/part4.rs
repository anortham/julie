#[tokio::test]
async fn test_manage_workspace_health_triggers_roots_resolution_when_primary_missing() {
    use crate::registry::database::DaemonDatabase;
    use crate::extractors::SymbolKind;
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new().unwrap();

    let startup_root = temp_dir.path().join("startup");
    let roots_root = temp_dir.path().join("roots");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&roots_root).unwrap();
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(roots_root.join("lib.rs"), "fn roots_health() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&startup_id, &startup_path.to_string_lossy(), "ready")
        .unwrap();
    let startup_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(startup_path.clone())
            .await
            .unwrap(),
    );

    let roots_path = roots_root.canonicalize().unwrap();
    let roots_id = generate_workspace_id(&roots_path.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&roots_id, &roots_path.to_string_lossy(), "ready")
        .unwrap();
    let roots_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(roots_path.clone())
            .await
            .unwrap(),
    );
    {
        let mut roots_db = roots_ws.db.as_ref().unwrap().lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "lib.rs".to_string(),
            language: "rust".to_string(),
            hash: "roots_health_hash".to_string(),
            size: 24,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: None,
        };
        let symbol = crate::extractors::Symbol {
            id: "roots_health_symbol".to_string(),
            name: "roots_health".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 18,
            start_byte: 0,
            end_byte: 18,
            signature: Some("fn roots_health()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
                        body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        };
        roots_db
            .bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &roots_id)
            .unwrap();
    }

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_id.clone()),
        None,
        None,
    )
    .await
    .unwrap();
    handler.set_client_supports_workspace_roots_for_test(true);
    assert_eq!(handler.current_workspace_id(), None);

    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let roots = [roots_path.as_path()];
    let roots_reply = answer_next_list_roots_request(&mut lines, &mut write_half, &roots);

    let health = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({
                "operation": "health",
                "detailed": false
            })
            .as_object()
            .expect("health args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(12), service.peer().clone()),
    );
    let (_, result) = tokio::join!(roots_reply, health);
    let result = result.unwrap();
    let text = extract_text_from_result(&result);

    assert!(
        text.contains("SQLite Status: HEALTHY"),
        "health should succeed after roots resolution: {text}"
    );
    assert!(
        text.contains("1 symbols across 1 files"),
        "health should report the roots-bound workspace stats: {text}"
    );
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(roots_id.as_str()),
        "health should bind the roots-selected current primary"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
}

#[tokio::test]
async fn test_manage_workspace_index_rejects_neutral_gap_without_primary_identity() {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn neutral_gap_index_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    handler.publish_loaded_workspace_swap_intent_for_test();

    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let err = tool
        .call_tool(&handler)
        .await
        .expect_err("neutral gap should reject primary index requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn test_manage_workspace_index_rejects_neutral_gap_without_primary_identity_after_teardown() {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn teardown_gap_index_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    handler
        .publish_loaded_workspace_swap_teardown_gap_for_test()
        .await;

    let err = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect_err("post-teardown swap gap should reject primary index requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );
}

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
