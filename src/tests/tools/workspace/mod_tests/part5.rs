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
    // NOTE: Avoid macro invocations (e.g. println!) because the Rust extractor captures
    // them as symbols, inflating counts beyond the intended function-only assertions.
    let test_file_1 = temp_dir.path().join("file1.rs");
    fs::write(
        &test_file_1,
        r#"
fn function_one() {
    let _ = 1;
}
        "#,
    )
    .unwrap();

    let test_file_2 = temp_dir.path().join("file2.rs");
    fs::write(
        &test_file_2,
        r#"
fn function_two() {
    let _ = 2;
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

    assert_eq!(
        primary_symbol_count(&handler).await,
        2,
        "Should have 2 symbols from 2 functions"
    );

    clear_primary_paths(&handler).await;
    assert_eq!(
        primary_symbol_count(&handler).await,
        0,
        "Store should have 0 symbols after clearing paths"
    );

    *handler.is_indexed.write().await = false;

    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("Workspace indexing complete"),
        "Re-indexing should complete"
    );

    assert_eq!(
        primary_symbol_count(&handler).await,
        2,
        "Incremental indexing should detect an empty store and re-extract symbols, got {} symbols",
        primary_symbol_count(&handler).await
    );
}

#[tokio::test]
async fn test_incremental_indexing_forces_reindex_when_index_engine_version_is_stale() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() { beta(); }\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

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
        message.contains("Workspace indexing complete"),
        "initial index should succeed: {message}"
    );

    let initial_edges = primary_store(&handler).await.status().graph.edges;
    assert!(
        initial_edges > 0,
        "test fixture should produce at least one relationship"
    );

    clear_primary_paths(&handler).await;
    assert_eq!(
        primary_symbol_count(&handler).await,
        0,
        "clearing paths should drop graph symbols before rebuild"
    );

    let incremental_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = incremental_tool.call_tool(&handler).await.unwrap();
    let message = extract_text_from_result(&result);
    assert!(
        message.contains("Workspace indexing complete"),
        "incremental index should complete: {message}"
    );

    assert_eq!(
        primary_store(&handler).await.status().graph.edges,
        initial_edges,
        "empty-store reindex should rebuild relationship edges"
    );
}

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_startup_empty_database_repair_runs_embeddings_after_initial_index() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    handler.set_injected_embedding_provider(Some(Arc::new(NoopEmbeddingProvider)));

    assert_eq!(
        embedding_count_for_primary(&handler).await,
        0,
        "fresh workspace should start without embeddings"
    );

    let plan = run_primary_workspace_repair(&handler)
        .await
        .unwrap()
        .expect("empty database should produce a startup repair plan");
    assert!(
        plan.reasons.contains(
            &crate::tools::workspace::indexing::state::IndexingRepairReason::EmptyDatabase
        ),
        "startup repair should report an empty database"
    );

    wait_for_embedding_tasks_to_finish(&handler).await;
    assert!(
        embedding_count_for_primary(&handler).await > 0,
        "startup empty-database repair should embed the newly indexed symbols"
    );
}

#[cfg(any())]
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_startup_stale_file_repair_refreshes_embeddings_for_changed_file() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() -> i32 { 1 }\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    handler.set_injected_embedding_provider(Some(Arc::new(BatchMarkerEmbeddingProvider::default())));

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await.unwrap();
    wait_for_embedding_tasks_to_finish(&handler).await;
    assert_eq!(
        first_embedding_value_for_symbol(&handler, "alpha").await,
        1.0,
        "initial embedding should come from the first embedding batch"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    fs::write(&test_file, "fn alpha() -> i64 { 1 }\n").unwrap();

    let plan = run_primary_workspace_repair(&handler)
        .await
        .unwrap()
        .expect("stale file should produce a startup repair plan");
    assert!(
        plan.reasons
            .contains(&crate::tools::workspace::indexing::state::IndexingRepairReason::StaleFiles),
        "startup repair should report stale files"
    );

    wait_for_embedding_tasks_to_finish(&handler).await;
    assert_eq!(
        first_embedding_value_for_symbol(&handler, "alpha").await,
        2.0,
        "startup stale-file repair should refresh the changed file in a second embedding batch"
    );
}
