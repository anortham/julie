use super::*;

#[tokio::test]
async fn test_manage_workspace_stats_explicit_target_succeeds_without_bound_primary_in_deferred_session()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let startup_root = temp_dir.path().join("startup");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
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
        Some(startup_id),
        None,
        None,
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats(&target_id, 17, 4, None, None, None)
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-target stats should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&format!("Workspace Statistics: {target_id}")),
        "explicit-target stats should return the requested workspace: {text}"
    );
    assert!(
        text.contains("Files: 4 | Symbols: 17"),
        "explicit-target stats should use target workspace stats: {text}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "explicit-target stats must not bind the deferred primary workspace"
    );
}

#[tokio::test]
async fn test_manage_workspace_refresh_by_workspace_id_succeeds_without_bound_primary_in_deferred_session()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let startup_root = temp_dir.path().join("startup");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
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
        Some(startup_id),
        None,
        None,
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-target refresh should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&format!("Workspace Refresh: {target_id}")),
        "explicit-target refresh should return the requested workspace: {text}"
    );
    assert!(
        !text.contains("Workspace Refresh Failed"),
        "explicit-target refresh should not fail in a deferred session: {text}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "explicit-target refresh must not bind the deferred primary workspace"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_by_workspace_id_succeeds_without_bound_primary_in_deferred_session()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let startup_root = temp_dir.path().join("startup");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
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
        Some(startup_id),
        None,
        None,
    )
    .await
    .expect("handler should initialize");
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let result = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("explicit-target open should succeed without a currently bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("Workspace Opened") && text.contains(&format!("Workspace ID: {target_id}")),
        "explicit-target open should return the requested workspace: {text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "known workspace should be active after explicit-target open"
    );
}
