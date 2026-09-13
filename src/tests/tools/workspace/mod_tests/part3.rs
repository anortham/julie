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
        let store = &rebound_ws.store;
        let bytes = fs::read(rebound_primary_path.join("lib.rs")).unwrap();
        let guard =
            julie_core::workspace::mutation_gate::acquire_gate(&rebound_primary_id).await;
        store
            .apply(
                &[julie_index::checkout_store::PathChange::Upsert {
                    path: "lib.rs".into(),
                    bytes,
                    language: "rust".into(),
                }],
                &guard,
            )
            .unwrap();
    }

    handler.set_current_primary_binding(rebound_primary_id.clone(), rebound_primary_path);

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: Some(rebound_primary_id),
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
        let store = &rebound_ws.store;
        let bytes = fs::read(rebound_primary_path.join("lib.rs")).unwrap();
        let guard =
            julie_core::workspace::mutation_gate::acquire_gate(&rebound_primary_id).await;
        store
            .apply(
                &[julie_index::checkout_store::PathChange::Upsert {
                    path: "lib.rs".into(),
                    bytes,
                    language: "rust".into(),
                }],
                &guard,
            )
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
            && report.contains("Workspace: rebound-primary-detailed_"),
        "detailed health should use rebound current-primary store status: {report}"
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
        SystemStatus::FullyReady { symbol_count } => assert!(symbol_count > 0),
        other => panic!(
            "loaded snapshot already has Tantivy, so readiness is FullyReady, got {other:?}"
        ),
    }
}

#[tokio::test]
async fn test_manage_workspace_health_cold_start_returns_index_first_guidance() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    let workspace_id = crate::workspace::registry::generate_workspace_id(&workspace_path).unwrap();
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: Some(workspace_id),
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
