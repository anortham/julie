#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_refresh_treats_semantic_version_drift_as_full_reindex() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() { beta(); }\nfn beta() {}\n").unwrap();

    let daemon_db_dir = temp_dir.path().join(".julie");
    fs::create_dir_all(&daemon_db_dir).unwrap();
    let daemon_db = Arc::new(
        crate::registry::database::DaemonDatabase::open(&daemon_db_dir.join("daemon.db")).unwrap(),
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
        crate::registry::database::DaemonDatabase::open(&daemon_db_dir.join("daemon.db")).unwrap(),
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
