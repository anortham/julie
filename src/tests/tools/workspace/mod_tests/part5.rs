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

    let workspace_id = handler
        .current_workspace_id()
        .expect("test handler should have a workspace id");
    let initial_relationships = {
        let workspace = handler
            .get_workspace()
            .await
            .unwrap()
            .expect("workspace should be initialized");
        let db = workspace.db.as_ref().expect("workspace db should exist");
        let db_lock = db.lock().unwrap();
        let _initial_revision = db_lock
            .get_current_canonical_revision(&workspace_id)
            .unwrap()
            .expect("initial index should record a canonical revision");
        let initial_relationships: i64 = db_lock
            .conn
            .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))
            .unwrap();
        assert!(
            initial_relationships > 0,
            "test fixture should produce at least one relationship"
        );
        initial_relationships
    };

    {
        let workspace = handler
            .get_workspace()
            .await
            .unwrap()
            .expect("workspace should be initialized");
        let db = workspace.db.as_ref().expect("workspace db should exist");
        let db_lock = db.lock().unwrap();
        db_lock
            .conn
            .execute("DELETE FROM relationships", [])
            .unwrap();
        let relationship_count: i64 = db_lock
            .conn
            .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            relationship_count, 0,
            "manual corruption should remove relationship rows before stale-version repair"
        );
        db_lock
            .set_index_engine_version(
                &workspace_id,
                SEMANTIC_INDEX_ENGINE_COMPONENT,
                "stale-test-version",
            )
            .unwrap();
    }

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

    let workspace = handler
        .get_workspace()
        .await
        .unwrap()
        .expect("workspace should be initialized");
    let db = workspace.db.as_ref().expect("workspace db should exist");
    let db_lock = db.lock().unwrap();
    let updated_revision = db_lock
        .get_current_canonical_revision(&workspace_id)
        .unwrap()
        .expect("stale engine reindex should record a canonical revision");
    assert!(
        updated_revision > 0,
        "stale semantic engine repair should leave a valid canonical revision"
    );

    let restored_relationships: i64 = db_lock
        .conn
        .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        restored_relationships, initial_relationships,
        "stale semantic engine version should rebuild derived relationship rows"
    );

    let stored_version = db_lock
        .get_index_engine_version(&workspace_id, SEMANTIC_INDEX_ENGINE_COMPONENT)
        .unwrap()
        .expect("successful reindex should store the current semantic engine version");
    assert_ne!(
        stored_version, "stale-test-version",
        "successful reindex should update the stored semantic engine version"
    );
    assert_eq!(
        stored_version, SEMANTIC_INDEX_ENGINE_VERSION,
        "stored semantic engine version should match the current code stamp"
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

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

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

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(BatchMarkerEmbeddingProvider::default()));
    }

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
