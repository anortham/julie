use super::*;

#[tokio::test]
async fn test_manage_workspace_list_includes_loaded_primary_without_explicit_registration() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let primary_root = temp_dir.path().join("primary");
    fs::create_dir_all(&primary_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();

    let handler = JulieServerHandler::new_deferred_daemon_startup_hint_without_project_log(
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: primary_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cli),
        },
        Some(Arc::clone(&daemon_db)),
        None,
    )
    .await
    .expect("handler should initialize");

    handler
        .ensure_workspace()
        .await
        .expect("primary workspace should load");

    let row = daemon_db
        .get_workspace(&primary_id)
        .expect("registry lookup should succeed")
        .expect("loaded primary should be registered for dashboard/list visibility");
    assert_eq!(row.path, primary_path_str);

    let result = ManageWorkspaceTool {
        operation: "list".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("list should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&format!("({}) [CURRENT]", primary_id)),
        "list should show the loaded primary workspace as CURRENT: {text}"
    );
    assert!(
        !text.contains("No workspaces registered"),
        "list must not hide the loaded primary workspace: {text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_list_labels_current_active_and_known_workspaces() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let primary_root = temp_dir.path().join("primary");
    let active_root = temp_dir.path().join("active");
    let known_root = temp_dir.path().join("known");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&active_root).unwrap();
    fs::create_dir_all(&known_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(active_root.join("lib.rs"), "fn active() {}\n").unwrap();
    fs::write(known_root.join("lib.rs"), "fn known() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();

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
    )
    .await
    .expect("handler should initialize");

    let active_path = active_root.canonicalize().unwrap();
    let active_path_str = active_path.to_string_lossy().to_string();
    let active_id = generate_workspace_id(&active_path_str).unwrap();
    daemon_db
        .upsert_workspace(&active_id, &active_path_str, "ready")
        .unwrap();
    handler.mark_workspace_active(&active_id);

    let known_path = known_root.canonicalize().unwrap();
    let known_path_str = known_path.to_string_lossy().to_string();
    let known_id = generate_workspace_id(&known_path_str).unwrap();
    daemon_db
        .upsert_workspace(&known_id, &known_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "list".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("list should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&primary_id),
        "list should include current workspace: {text}"
    );
    assert!(
        text.contains(&active_id),
        "list should include active workspace: {text}"
    );
    assert!(
        text.contains(&known_id),
        "list should include known workspace: {text}"
    );
    assert!(
        text.contains("CURRENT"),
        "list should annotate current workspace: {text}"
    );
    assert!(
        text.contains("ACTIVE"),
        "list should annotate active workspace: {text}"
    );
    assert!(
        text.contains("KNOWN"),
        "list should annotate inactive known workspace: {text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_list_uses_session_primary_binding_for_current_label() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let legacy_primary_root = temp_dir.path().join("legacy-primary");
    let rebound_primary_root = temp_dir.path().join("rebound-primary");
    let active_root = temp_dir.path().join("active");
    fs::create_dir_all(&legacy_primary_root).unwrap();
    fs::create_dir_all(&rebound_primary_root).unwrap();
    fs::create_dir_all(&active_root).unwrap();
    fs::write(
        legacy_primary_root.join("main.rs"),
        "fn legacy_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();
    fs::write(active_root.join("lib.rs"), "fn active() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let legacy_primary_path = legacy_primary_root.canonicalize().unwrap();
    let legacy_primary_path_str = legacy_primary_path.to_string_lossy().to_string();
    let legacy_primary_id = generate_workspace_id(&legacy_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&legacy_primary_id, &legacy_primary_path_str, "ready")
        .unwrap();

    let legacy_primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(legacy_primary_path.clone())
            .await
            .expect("legacy primary workspace should initialize"),
    );
    let handler = JulieServerHandler::new_with_shared_workspace(
        legacy_primary_ws,
        legacy_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(legacy_primary_id.clone()),
        None,
    )
    .await
    .expect("handler should initialize");

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();

    let active_path = active_root.canonicalize().unwrap();
    let active_path_str = active_path.to_string_lossy().to_string();
    let active_id = generate_workspace_id(&active_path_str).unwrap();
    daemon_db
        .upsert_workspace(&active_id, &active_path_str, "ready")
        .unwrap();

    handler.set_current_primary_binding(rebound_primary_id.clone(), rebound_primary_path);
    handler.mark_workspace_active(&active_id);

    let result = ManageWorkspaceTool {
        operation: "list".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("list should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&format!("({}) [CURRENT]", rebound_primary_id)),
        "list should mark rebound session primary as CURRENT: {text}"
    );
    assert!(
        text.contains(&format!("({}) [ACTIVE]", active_id)),
        "list should mark the secondary active workspace as ACTIVE: {text}"
    );
    assert!(
        text.contains(&format!("({}) [KNOWN]", legacy_primary_id)),
        "legacy workspace_id should no longer drive CURRENT labeling: {text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_stats_include_all_known_workspaces() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let primary_root = temp_dir.path().join("primary");
    let active_root = temp_dir.path().join("active");
    let known_root = temp_dir.path().join("known");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&active_root).unwrap();
    fs::create_dir_all(&known_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(active_root.join("lib.rs"), "fn active() {}\n").unwrap();
    fs::write(known_root.join("lib.rs"), "fn known() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_path_str = primary_path.to_string_lossy().to_string();
    let primary_id = generate_workspace_id(&primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&primary_id, 10, 2, None, None, None)
        .unwrap();

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
    )
    .await
    .expect("handler should initialize");

    let active_path = active_root.canonicalize().unwrap();
    let active_path_str = active_path.to_string_lossy().to_string();
    let active_id = generate_workspace_id(&active_path_str).unwrap();
    daemon_db
        .upsert_workspace(&active_id, &active_path_str, "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&active_id, 20, 3, None, None, None)
        .unwrap();
    handler.mark_workspace_active(&active_id);

    let known_path = known_root.canonicalize().unwrap();
    let known_path_str = known_path.to_string_lossy().to_string();
    let known_id = generate_workspace_id(&known_path_str).unwrap();
    daemon_db
        .upsert_workspace(&known_id, &known_path_str, "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&known_id, 30, 5, None, None, None)
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("stats should succeed");

    let text = extract_text_from_result(&result);
    let active_workspace_count = handler.active_workspace_ids().await.len();
    assert!(
        text.contains("Known Workspaces: 3"),
        "stats should count all known workspaces: {text}"
    );
    assert!(
        text.contains(&format!(
            "Active Workspaces In Session: {}",
            active_workspace_count
        )),
        "stats should report the session's active workspace count: {text}"
    );
    assert!(
        text.contains("Total Files: 10"),
        "old paired-only total should fail before fix: {text}"
    );
    assert!(
        text.contains("Total Symbols: 60"),
        "stats should aggregate all known symbols: {text}"
    );
}
