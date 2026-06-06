use super::*;

#[tokio::test]
async fn test_manage_workspace_open_uses_session_primary_binding_over_legacy_workspace_id() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = make_isolated_workspace_root(temp_dir.path(), "primary");
    let target_root = make_isolated_workspace_root(temp_dir.path(), "target");
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    let primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(primary_path.clone())
            .await
            .expect("primary workspace should initialize"),
    );

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    handler.set_current_primary_binding(target_id.clone(), target_path.clone());

    let stats_tool = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let stats_result = stats_tool.call_tool(&handler).await.unwrap();
    let stats_text = extract_text_from_result(&stats_result);
    assert!(
        stats_text.contains(&format!("Current Workspace: {}", target_id)),
        "stats should use session primary binding, not stale workspace_id: {stats_text}"
    );

    let renamed_target = temp_dir.path().join("target-renamed");
    fs::rename(&target_root, &renamed_target).unwrap();

    let open_tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };
    let open_result = open_tool.call_tool(&handler).await.unwrap();
    let open_text = extract_text_from_result(&open_result);
    assert!(
        open_text.contains("Workspace Pruned"),
        "open should prune a rebound current workspace when its path is gone: {open_text}"
    );
    assert!(
        !open_text.contains("Workspace Refresh Failed"),
        "open should not fall back to the old refresh failure text: {open_text}"
    );
}

#[ignore = "daemon multi-workspace write lifecycle (pool-backed); fate decided in Phase 3d.3 registry rework"]
#[tokio::test]
async fn test_manage_workspace_index_path_rebind_updates_daemon_stats_for_new_primary() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let workspace_a_root = make_isolated_workspace_root(temp_dir.path(), "workspace-a");
    let workspace_b_root = make_isolated_workspace_root(temp_dir.path(), "workspace-b");
    fs::write(workspace_a_root.join("main.rs"), "fn workspace_a() {}\n").unwrap();
    fs::write(workspace_b_root.join("lib.rs"), "fn workspace_b() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let workspace_a_path = workspace_a_root.canonicalize().unwrap();
    let workspace_a_path_str = workspace_a_path.to_string_lossy().to_string();
    let workspace_a_id = generate_workspace_id(&workspace_a_path_str).unwrap();
    let workspace_a_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(workspace_a_path.clone())
            .await
            .expect("workspace A should initialize"),
    );

    let handler = JulieServerHandler::new_with_shared_workspace(
        workspace_a_ws,
        workspace_a_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(workspace_a_id.clone()),
        None,
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    let workspace_b_path = workspace_b_root.canonicalize().unwrap();
    let workspace_b_path_str = workspace_b_path.to_string_lossy().to_string();
    let workspace_b_id = generate_workspace_id(&workspace_b_path_str).unwrap();
    daemon_db
        .upsert_workspace(&workspace_a_id, &workspace_a_path_str, "ready")
        .unwrap();
    let index_result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_b_path_str.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("index(path=B) should perform the rebind");
    let index_text = extract_text_from_result(&index_result);
    assert!(
        index_text.contains("Workspace indexing complete")
            || index_text.contains("Workspace already indexed"),
        "index(path=B) should complete: {index_text}"
    );

    assert_eq!(handler.current_workspace_id(), Some(workspace_b_id.clone()));

    let workspace_b_row = daemon_db
        .get_workspace(&workspace_b_id)
        .unwrap()
        .expect("workspace B row should exist");
    assert_eq!(workspace_b_row.status, "ready");
    assert_eq!(workspace_b_row.session_count, 1);

    let workspace_a_row = daemon_db
        .get_workspace(&workspace_a_id)
        .unwrap()
        .expect("workspace A row should exist");
    assert_eq!(workspace_a_row.session_count, 1);
}

#[tokio::test]
async fn test_manage_workspace_index_path_succeeds_without_bound_primary_in_deferred_cwd_session_when_target_registered()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = make_isolated_workspace_root(temp_dir.path(), "startup");
    let target_root = make_isolated_workspace_root(temp_dir.path(), "target");
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    let startup_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(startup_path.clone())
            .await
            .expect("startup workspace should initialize"),
    );

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
        None,
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_id = generate_workspace_id(&target_path.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path.to_string_lossy(), "ready")
        .unwrap();
    let result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(target_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-path index should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete") || text.contains("Workspace already indexed"),
        "explicit-path index should complete: {text}"
    );
    assert_eq!(handler.current_workspace_id(), Some(target_id.clone()));

    let target_row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("target workspace row should exist after explicit-path index");
    assert_eq!(target_row.status, "ready");
}

#[tokio::test]
async fn test_manage_workspace_index_path_succeeds_without_bound_primary_in_deferred_cwd_session_when_target_unregistered()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = make_isolated_workspace_root(temp_dir.path(), "startup");
    let target_root = make_isolated_workspace_root(temp_dir.path(), "target");
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    let startup_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(startup_path.clone())
            .await
            .expect("startup workspace should initialize"),
    );

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
        None,
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_id = generate_workspace_id(&target_path.to_string_lossy()).unwrap();
    let result = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(target_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-path index should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace indexing complete") || text.contains("Workspace already indexed"),
        "explicit-path index should complete: {text}"
    );
    assert_eq!(handler.current_workspace_id(), Some(target_id.clone()));

    let target_row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("target workspace row should exist after explicit-path index");
    assert_eq!(target_row.status, "ready");
}
