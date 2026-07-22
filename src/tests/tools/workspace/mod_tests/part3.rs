#[tokio::test]
async fn test_manage_workspace_health_uses_rebound_session_primary() {
    use crate::registry::database::DaemonDatabase;
    use crate::workspace::registry::generate_workspace_id;

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let loaded_primary_root = temp_dir.path().join("loaded-primary");
    let rebound_primary_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(&loaded_primary_root).unwrap();
    fs::create_dir_all(&rebound_primary_root).unwrap();
    fs::write(
        loaded_primary_root.join("main.rs"),
        "fn loaded_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let loaded_primary_path = loaded_primary_root.canonicalize().unwrap();
    let loaded_primary_path_str = loaded_primary_path.to_string_lossy().to_string();
    let loaded_primary_id = generate_workspace_id(&loaded_primary_path_str).unwrap();
    let loaded_primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(loaded_primary_path.clone())
            .await
            .unwrap(),
    );

    let handler = JulieServerHandler::new_with_shared_workspace(
        loaded_primary_ws,
        loaded_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(loaded_primary_id.clone()),
        None,
        None,
    )
    .await
    .unwrap();
    {
        let mut loaded_workspace = handler.workspace.write().await;
        loaded_workspace
            .as_mut()
            .expect("loaded workspace should exist")
            .search_index = None;
    }

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&loaded_primary_id, &loaded_primary_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();

    let rebound_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(rebound_primary_path.clone())
            .await
            .unwrap(),
    );
    {
        let mut rebound_guard = rebound_ws.db.as_ref().unwrap().lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "lib.rs".to_string(),
            language: "rust".to_string(),
            hash: "rebound_hash".to_string(),
            size: 32,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: None,
        };
        let symbol = crate::extractors::Symbol {
            id: "rebound_symbol".to_string(),
            name: "rebound_primary".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 20,
            start_byte: 0,
            end_byte: 20,
            signature: Some("fn rebound_primary()".to_string()),
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
        rebound_guard
            .bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &rebound_primary_id)
            .unwrap();
    }

    handler.set_current_primary_binding(rebound_primary_id, rebound_primary_path);

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);

    assert!(
        health.contains("SQLite Status: HEALTHY"),
        "health should use rebound current primary database: {health}"
    );
    assert!(
        health.contains("1 symbols across 1 files"),
        "health should report rebound primary stats, not stale loaded workspace stats: {health}"
    );
}

#[ignore = "daemon multi-workspace write lifecycle (pool-backed); fate decided in Phase 3d.3 registry rework"]
#[tokio::test(flavor = "multi_thread")]
async fn test_manage_workspace_health_keeps_primary_snapshot_after_completed_swap() {
    use crate::registry::database::DaemonDatabase;
    use crate::health::{HealthChecker, SystemStatus};
    use crate::workspace::registry::generate_workspace_id;
    use futures::poll;

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let original_root = temp_dir.path().join("loaded-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src")).unwrap();
    fs::create_dir_all(rebound_root.join("src")).unwrap();
    fs::write(
        original_root.join("src").join("main.rs"),
        "fn loaded_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_root.join("src").join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let original_path = original_root.canonicalize().unwrap();
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str).unwrap();
    daemon_db
        .upsert_workspace(&original_id, &original_path_str, "ready")
        .unwrap();
    let original_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(original_path.clone())
            .await
            .unwrap(),
    );
    let original_meta_path = indexes_dir
        .join(&original_id)
        .join("tantivy")
        .join("meta.json");
    if original_meta_path.exists() {
        fs::remove_file(&original_meta_path).unwrap();
    }
    let mut original_handler_ws = (*original_ws).clone();
    original_handler_ws.search_index = None;
    {
        let mut original_guard = original_ws.db.as_ref().unwrap().lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            hash: "original_hash".to_string(),
            size: 24,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: None,
        };
        let symbol = crate::extractors::Symbol {
            id: "original_symbol".to_string(),
            name: "loaded_primary".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/main.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 19,
            start_byte: 0,
            end_byte: 19,
            signature: Some("fn loaded_primary()".to_string()),
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
        original_guard
            .bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &original_id)
            .unwrap();
    }

    let handler = JulieServerHandler::new_with_shared_workspace(
        Arc::new(original_handler_ws),
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
    )
    .await
    .unwrap();

    let rebound_path = rebound_root.canonicalize().unwrap();
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str).unwrap();
    daemon_db
        .upsert_workspace(&rebound_id, &rebound_path_str, "ready")
        .unwrap();
    crate::workspace::JulieWorkspace::initialize(rebound_path.clone())
        .await
        .unwrap();

    let workspace_write_guard = handler.workspace.write().await;
    let mut readiness_future = Box::pin(HealthChecker::check_system_readiness(&handler, None));
    assert!(
        poll!(readiness_future.as_mut()).is_pending(),
        "health check should block on the first await while the workspace lock is held"
    );

    handler.set_current_primary_binding(rebound_id, rebound_path);
    drop(workspace_write_guard);
    assert!(
        !handler.is_primary_workspace_swap_in_progress(),
        "swap should be completed before the readiness future resumes"
    );

    match readiness_future.await.unwrap() {
        SystemStatus::SqliteOnly { symbol_count } => {
            assert_eq!(
                symbol_count, 1,
                "health should stay bound to the original snapshot"
            )
        }
        other => panic!("expected SqliteOnly from the original primary snapshot, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_manage_workspace_health_detailed_uses_rebound_session_primary() {
    use crate::registry::database::DaemonDatabase;
    use crate::health::HealthChecker;
    use crate::workspace::registry::generate_workspace_id;

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let loaded_primary_root = temp_dir.path().join("loaded-primary-detailed");
    let rebound_primary_root = temp_dir.path().join("rebound-primary-detailed");
    fs::create_dir_all(&loaded_primary_root).unwrap();
    fs::create_dir_all(&rebound_primary_root).unwrap();
    fs::write(
        loaded_primary_root.join("main.rs"),
        "fn loaded_primary_detailed() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary_detailed() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let loaded_primary_path = loaded_primary_root.canonicalize().unwrap();
    let loaded_primary_path_str = loaded_primary_path.to_string_lossy().to_string();
    let loaded_primary_id = generate_workspace_id(&loaded_primary_path_str).unwrap();
    let loaded_primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(loaded_primary_path.clone())
            .await
            .unwrap(),
    );

    let handler = JulieServerHandler::new_with_shared_workspace(
        loaded_primary_ws,
        loaded_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(loaded_primary_id.clone()),
        None,
        None,
    )
    .await
    .unwrap();

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&loaded_primary_id, &loaded_primary_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();

    let rebound_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(rebound_primary_path.clone())
            .await
            .unwrap(),
    );
    {
        let mut rebound_guard = rebound_ws.db.as_ref().unwrap().lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "lib.rs".to_string(),
            language: "rust".to_string(),
            hash: "rebound_detailed_hash".to_string(),
            size: 41,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: None,
        };
        let symbol = crate::extractors::Symbol {
            id: "rebound_detailed_symbol".to_string(),
            name: "rebound_primary_detailed".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 29,
            start_byte: 0,
            end_byte: 29,
            signature: Some("fn rebound_primary_detailed()".to_string()),
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
        rebound_guard
            .bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &rebound_primary_id)
            .unwrap();
    }

    handler.set_current_primary_binding(rebound_primary_id, rebound_primary_path);

    let report = HealthChecker::get_detailed_health_report(&handler)
        .await
        .unwrap();

    assert!(
        report.contains("📊 Database: 1 symbols, 1 files, 0 relationships"),
        "detailed health should use rebound current-primary stats, not the stale loaded workspace: {report}"
    );
    assert!(
        report.contains("Projection tantivy")
            && report.contains("Projection web_edges")
            && report.contains("Workspace: rebound-primary-detailed_")
            && report.contains("Freshness: REBUILD REQUIRED"),
        "detailed health should use rebound current-primary projection state instead of stale loaded workspace state: {report}"
    );
    assert!(
        report.contains("Indexed workspace languages (1): rust"),
        "detailed health should describe indexed workspace languages, not the tree-sitter support matrix: {report}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_loaded_primary_without_tantivy_is_sqlite_only() {
    use crate::health::{HealthChecker, SystemStatus};

    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn sqlite_only_loaded_primary() {}\n",
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

    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())
            .unwrap();
    let tantivy_dir = handler
        .workspace_tantivy_dir_for(&workspace_id)
        .await
        .unwrap();
    let meta_path = tantivy_dir.join("meta.json");
    if meta_path.exists() {
        fs::remove_file(meta_path).unwrap();
    }

    let readiness = HealthChecker::check_system_readiness(&handler, None)
        .await
        .unwrap();
    match readiness {
        SystemStatus::SqliteOnly { symbol_count } => assert!(symbol_count > 0),
        other => panic!("expected SqliteOnly for loaded primary without Tantivy, got {other:?}"),
    }
}

#[tokio::test]
async fn test_manage_workspace_health_rejects_neutral_gap_without_primary_identity() {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn neutral_gap_health_target() {}\n",
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
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let err = tool
        .call_tool(&handler)
        .await
        .expect_err("neutral gap should reject primary health requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_cold_start_returns_index_first_guidance() {
    let handler = JulieServerHandler::new_for_test().await.unwrap();

    let result = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    }
    .call_tool(&handler)
    .await
    .unwrap();

    let health = extract_text_from_result(&result);

    assert!(
        health
            .contains("No workspace initialized. Run manage_workspace(operation=\"index\") first."),
        "cold start should keep index-first guidance, got: {health}"
    );
    assert!(
        !health.contains("Primary workspace identity unavailable during swap"),
        "cold start should not be classified as a swap gap: {health}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_true_swap_gap_uses_swap_gap_classification() {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn true_swap_gap_health_target() {}\n",
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
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    }
    .call_tool(&handler)
    .await
    .expect_err("true swap gap should reject primary health requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );
}
