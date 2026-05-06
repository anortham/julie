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

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_startup_semantic_repair_runs_embeddings_after_full_reindex() {
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

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let result = index_tool.call_tool(&handler).await.unwrap();
    let message = extract_text_from_result(&result);
    assert!(
        message.contains("Workspace indexing complete"),
        "initial index should succeed: {message}"
    );
    wait_for_embedding_tasks_to_finish(&handler).await;
    assert!(
        embedding_count_for_primary(&handler).await > 0,
        "initial index should embed symbols before the semantic drift repair"
    );

    let workspace_id = handler
        .current_workspace_id()
        .expect("test handler should have a workspace id");
    {
        let workspace = handler
            .get_workspace()
            .await
            .unwrap()
            .expect("workspace should be initialized");
        let db = workspace.db.as_ref().expect("workspace db should exist");
        let db_lock = db.lock().unwrap();
        db_lock
            .set_index_engine_version(
                &workspace_id,
                SEMANTIC_INDEX_ENGINE_COMPONENT,
                "stale-startup-test-version",
            )
            .unwrap();
    }

    let plan = run_primary_workspace_repair(&handler)
        .await
        .unwrap()
        .expect("semantic drift should produce a startup repair plan");
    assert!(
        plan.reasons.contains(
            &crate::tools::workspace::indexing::state::IndexingRepairReason::SemanticVersionChanged
        ),
        "startup repair should report semantic-version drift"
    );

    wait_for_embedding_tasks_to_finish(&handler).await;
    assert!(
        embedding_count_for_primary(&handler).await > 0,
        "startup semantic full reindex should catch embeddings back up even though auto-index normally skips them"
    );
}

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_refresh_treats_semantic_version_drift_as_full_reindex() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() { beta(); }\nfn beta() {}\n").unwrap();

    let daemon_db_dir = temp_dir.path().join(".julie");
    fs::create_dir_all(&daemon_db_dir).unwrap();
    let daemon_db = Arc::new(
        crate::daemon::database::DaemonDatabase::open(&daemon_db_dir.join("daemon.db")).unwrap(),
    );

    let workspace_path_str = temp_dir.path().to_string_lossy().to_string();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path_str).unwrap();

    let mut handler = JulieServerHandler::new_for_test().await.unwrap();
    handler.daemon_db = Some(daemon_db.clone());
    *handler
        .workspace_id
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(workspace_id.clone());
    handler
        .initialize_workspace_with_force(Some(workspace_path_str.clone()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    daemon_db
        .upsert_workspace(&workspace_id, &workspace_path_str, "ready")
        .unwrap();

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path_str.clone()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let result = index_tool.call_tool(&handler).await.unwrap();
    let message = extract_text_from_result(&result);
    assert!(
        message.contains("Workspace indexing complete"),
        "initial index should succeed: {message}"
    );
    wait_for_embedding_tasks_to_finish(&handler).await;

    {
        let workspace = handler
            .get_workspace()
            .await
            .unwrap()
            .expect("workspace should be initialized");
        let db = workspace.db.as_ref().expect("workspace db should exist");
        let db_lock = db.lock().unwrap();
        db_lock
            .set_index_engine_version(
                &workspace_id,
                SEMANTIC_INDEX_ENGINE_COMPONENT,
                "stale-refresh-test-version",
            )
            .unwrap();
    }

    let refresh_tool = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(workspace_id.clone()),
        detailed: None,
    };
    let result = refresh_tool.call_tool(&handler).await.unwrap();
    let message = extract_text_from_result(&result);

    assert!(
        message.contains("Full re-index"),
        "semantic-version drift should be orchestrated as an effective full reindex: {message}"
    );
}

#[tokio::test]
#[serial]
async fn test_force_reindex_cancels_embedding_task_when_explicit_path_resolves_to_primary_root() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname = \"workspace-test\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    let src_dir = temp_dir.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("main.rs"), "fn alpha() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    let workspace_id = handler
        .current_workspace_id()
        .expect("test handler should have a primary workspace id");
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let pending_handle = tokio::spawn(async {
        std::future::pending::<()>().await;
    });
    {
        let mut tasks = handler.embedding_tasks.lock().await;
        tasks.insert(
            workspace_id.clone(),
            (Arc::clone(&cancel_flag), pending_handle),
        );
    }

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(src_dir.to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let result = index_tool
        .call_tool_with_options(&handler, true)
        .await
        .unwrap();
    let message = extract_text_from_result(&result);
    assert!(
        message.contains("Workspace indexing complete"),
        "explicit child-path force index should resolve to the workspace root: {message}"
    );
    assert!(
        cancel_flag.load(Ordering::Acquire),
        "force reindex should cancel the embedding task keyed by the resolved primary workspace id"
    );
    let mut tasks = handler.embedding_tasks.lock().await;
    if let Some((stored_flag, handle)) = tasks.remove(&workspace_id) {
        assert!(
            !Arc::ptr_eq(&stored_flag, &cancel_flag),
            "force reindex should not leave the stale embedding task keyed by the resolved primary workspace id"
        );
        handle.abort();
    }
}

#[test]
fn test_force_reindex_workspace_ids_include_primary_and_canonical_ids() {
    let temp_dir = TempDir::new().unwrap();
    let canonical_id =
        crate::workspace::registry::generate_workspace_id(&temp_dir.path().to_string_lossy())
            .unwrap();
    let primary_id = "primary_alias_id";

    let workspace_ids =
        crate::tools::workspace::commands::force_safeguards::workspace_ids_for_force_reindex(
            temp_dir.path(),
            Some(primary_id),
            false,
        )
        .unwrap();

    assert!(
        workspace_ids.iter().any(|id| id == primary_id),
        "primary force safeguards should include the currently bound primary id"
    );
    assert!(
        workspace_ids.iter().any(|id| id == &canonical_id),
        "primary force safeguards should include the canonical path id"
    );
}

#[test]
fn test_force_reindex_workspace_ids_exclude_primary_id_for_non_primary_target() {
    let temp_dir = TempDir::new().unwrap();
    let canonical_id =
        crate::workspace::registry::generate_workspace_id(&temp_dir.path().to_string_lossy())
            .unwrap();

    let workspace_ids =
        crate::tools::workspace::commands::force_safeguards::workspace_ids_for_force_reindex(
            temp_dir.path(),
            Some("primary_alias_id"),
            true,
        )
        .unwrap();

    assert_eq!(
        workspace_ids,
        vec![canonical_id],
        "non-primary force safeguards should only touch the target workspace id"
    );
}

/// Regression test: refresh with no file changes should NOT trigger the full
/// embedding pipeline. Previously, every refresh unconditionally called
/// spawn_workspace_embedding, re-embedding ~2000 enriched symbols even when
/// nothing changed.
#[tokio::test]
async fn test_refresh_no_changes_skips_embedding_pipeline() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn hello() {}\nfn world() {}\n").unwrap();

    // Set up daemon database (refresh requires daemon mode)
    let daemon_db_dir = temp_dir.path().join(".julie");
    fs::create_dir_all(&daemon_db_dir).unwrap();
    let daemon_db = Arc::new(
        crate::daemon::database::DaemonDatabase::open(&daemon_db_dir.join("daemon.db")).unwrap(),
    );

    let workspace_path_str = temp_dir.path().to_string_lossy().to_string();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path_str).unwrap();

    // Create handler with daemon_db
    let mut handler = JulieServerHandler::new_for_test().await.unwrap();
    handler.daemon_db = Some(daemon_db.clone());
    *handler
        .workspace_id
        .write()
        .unwrap_or_else(|p| p.into_inner()) = Some(workspace_id.clone());

    handler
        .initialize_workspace_with_force(Some(workspace_path_str.clone()), true)
        .await
        .unwrap();

    // Inject a real embedding provider so spawn_workspace_embedding would
    // return non-zero if called. Without this, the test could pass trivially
    // because no provider means embed_count=0 regardless of the gate.
    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    // Register workspace in daemon db
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_path_str, "ready")
        .unwrap();

    // First: index the workspace so files are known
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path_str.clone()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let result = index_tool.call_tool(&handler).await.unwrap();
    let msg = extract_text_from_result(&result);
    assert!(
        msg.contains("Workspace indexing complete"),
        "Index should succeed: {msg}"
    );

    // Wait for background embedding to finish so the workspace has embeddings
    // before refreshing. Without this, the catch-up logic would (correctly)
    // schedule embedding because the workspace has symbols but 0 vectors.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let tasks = handler.embedding_tasks.lock().await;
        if tasks.is_empty() {
            break;
        }
        drop(tasks);
        assert!(
            Instant::now() < deadline,
            "Embedding task did not complete within 5s"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Now: refresh with no changes and no force
    let refresh_tool = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(workspace_id.clone()),
        detailed: None,
    };
    let result = refresh_tool.call_tool(&handler).await.unwrap();
    let msg = extract_text_from_result(&result);

    assert!(
        msg.contains("Already up-to-date"),
        "Refresh with no changes should report up-to-date: {msg}"
    );
    // The bug: embedding pipeline was triggered even when nothing changed
    assert!(
        !msg.contains("Embedding"),
        "Refresh with no changes should NOT trigger embedding pipeline: {msg}"
    );
}

#[tokio::test]
#[serial]
async fn test_incremental_index_triggers_catch_up_embedding_when_none_exist() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    // Inject provider so embedding can run
    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    // First index with force: creates symbols and spawns background embedding
    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let _ = tool.call_tool(&handler).await.unwrap();

    // Wait for background embedding task to finish
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let tasks = handler.embedding_tasks.lock().await;
        if tasks.is_empty() {
            break;
        }
        drop(tasks);
        assert!(
            Instant::now() < deadline,
            "Embedding task did not complete within 5s"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Clear all embeddings to simulate "sidecar wasn't ready during initial indexing"
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let mut db_lock = db.lock().unwrap();
            db_lock.clear_all_embeddings().unwrap();
            assert_eq!(
                db_lock.embedding_count().unwrap(),
                0,
                "embeddings should be cleared"
            );
            assert!(
                db_lock.count_symbols_for_workspace().unwrap() > 0,
                "symbols should still exist"
            );
        }
    }

    // Second index: force=false, incremental, with no file changes detected
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
        message.contains("Embedding") && message.contains("background"),
        "Incremental index should schedule catch-up embedding when workspace has symbols but 0 embeddings. Message: {message}"
    );
}
