use super::*;

#[tokio::test]
async fn test_manage_workspace_open_registers_missing_workspace_and_returns_workspace_id() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn opened() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: Some(target_path_str.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let text = extract_text_from_result(&result);

    assert!(
        text.contains(&target_id),
        "open result should include workspace ID: {text}"
    );
    assert!(
        text.contains(&target_path_str),
        "open result should include canonical path: {text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "opened workspace should be active for the session"
    );

    let row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("workspace should be registered in daemon db");
    assert_eq!(row.path, target_path_str);
    assert_eq!(row.status, "ready");
}

#[tokio::test]
async fn test_manage_workspace_register_does_not_mutate_primary_binding_during_rebound_session() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let legacy_primary_root = temp_dir.path().join("legacy-primary");
    let rebound_primary_root = temp_dir.path().join("rebound-primary");
    let reference_root = temp_dir.path().join("reference");
    fs::create_dir_all(&legacy_primary_root).unwrap();
    fs::create_dir_all(&rebound_primary_root).unwrap();
    fs::create_dir_all(&reference_root).unwrap();
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
    fs::write(
        reference_root.join("lib.rs"),
        "pub fn reference_marker() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
    ));

    let legacy_primary_path = legacy_primary_root.canonicalize().unwrap();
    let legacy_primary_path_str = legacy_primary_path.to_string_lossy().to_string();
    let legacy_primary_id = generate_workspace_id(&legacy_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&legacy_primary_id, &legacy_primary_path_str, "ready")
        .unwrap();

    let legacy_primary_ws = pool
        .get_or_init(&legacy_primary_id, legacy_primary_path.clone())
        .await
        .expect("legacy primary workspace should initialize");
    let handler = JulieServerHandler::new_with_shared_workspace(
        legacy_primary_ws,
        legacy_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(legacy_primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();
    handler.set_current_primary_binding(rebound_primary_id.clone(), rebound_primary_path);

    let reference_path = reference_root.canonicalize().unwrap();
    let reference_path_str = reference_path.to_string_lossy().to_string();
    let reference_id = generate_workspace_id(&reference_path_str).unwrap();

    let result = ManageWorkspaceTool {
        operation: "register".to_string(),
        path: Some(reference_path_str.clone()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("register should succeed");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&reference_id),
        "register output should include workspace id: {text}"
    );
    assert!(
        handler.current_workspace_id().as_deref() == Some(rebound_primary_id.as_str()),
        "register should leave the rebound primary binding untouched"
    );
    assert!(
        !handler.active_workspace_ids().await.contains(&reference_id),
        "register should not activate the known workspace in the session"
    );

    let row = daemon_db
        .get_workspace(&reference_id)
        .unwrap()
        .expect("workspace should be registered in daemon db");
    assert_eq!(row.path, reference_path_str);
    assert_eq!(row.status, "ready");
}

#[tokio::test]
async fn test_manage_workspace_open_by_workspace_id_marks_known_workspace_active() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn known_target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();

    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let text = extract_text_from_result(&result);

    assert!(
        text.contains(&target_id),
        "open-by-id result should include workspace ID: {text}"
    );
    assert!(
        text.contains(&target_path_str),
        "open-by-id result should include workspace path: {text}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        Some(target_id.clone()),
        "open-by-id should switch default primary routing to the opened workspace"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "known workspace should be active after open"
    );
    assert!(
        handler.is_workspace_active(&primary_id).await,
        "switching primary with open should keep the previous primary active"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_does_not_activate_workspace_when_refresh_fails() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    fs::create_dir_all(&primary_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let missing_target = temp_dir.path().join("missing-target");
    let target_path_str = missing_target.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let text = extract_text_from_result(&result);

    assert!(
        text.contains("Workspace Pruned"),
        "open should surface prune text when the path is gone: {text}"
    );
    assert!(
        !handler.is_workspace_active(&target_id).await,
        "workspace should remain inactive when refresh fails"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_is_idempotent_for_active_workspace() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn known_target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path.clone())
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    tool.call_tool(&handler).await.unwrap();
    let first_row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("target workspace should exist after first open");
    assert_eq!(
        first_row.session_count, 1,
        "first open should attach the workspace once"
    );

    tool.call_tool(&handler).await.unwrap();
    let second_row = daemon_db
        .get_workspace(&target_id)
        .unwrap()
        .expect("target workspace should exist after second open");
    assert_eq!(
        second_row.session_count, 1,
        "second open in the same session must not increment session_count"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_short_circuits_when_active() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn known_target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path)
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_root.canonicalize().unwrap(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let first = tool.call_tool(&handler).await.unwrap();
    let first_text = extract_text_from_result(&first);
    assert!(first_text.contains("Workspace Opened"));
    assert!(handler.is_workspace_active(&target_id).await);

    let renamed_target = temp_dir.path().join("target-renamed");
    fs::rename(&target_root, &renamed_target).unwrap();

    let second = tool.call_tool(&handler).await.unwrap();
    let second_text = extract_text_from_result(&second);
    assert!(
        second_text.contains("Workspace Missing But Still Active"),
        "active workspace reopen should report blocked cleanup when the path is gone: {second_text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "workspace should remain active after blocked cleanup"
    );
}

#[tokio::test]
async fn test_manage_workspace_open_force_active_workspace_runs_refresh() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    fs::write(target_root.join("lib.rs"), "fn known_target() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
    ));

    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = pool
        .get_or_init(&primary_id, primary_path)
        .await
        .expect("primary workspace should initialize");

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_root.canonicalize().unwrap(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .expect("handler should initialize");

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let open_tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let initial = open_tool.call_tool(&handler).await.unwrap();
    let initial_text = extract_text_from_result(&initial);
    assert!(initial_text.contains("Workspace Opened"));
    assert!(handler.is_workspace_active(&target_id).await);

    let renamed_target = temp_dir.path().join("target-renamed");
    fs::rename(&target_root, &renamed_target).unwrap();

    let force_open_tool = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    };

    let forced = force_open_tool.call_tool(&handler).await.unwrap();
    let forced_text = extract_text_from_result(&forced);
    assert!(
        forced_text.contains("Workspace Missing But Still Active"),
        "force open should surface blocked-cleanup text: {forced_text}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "failed force refresh should not silently deactivate the active workspace"
    );
}
