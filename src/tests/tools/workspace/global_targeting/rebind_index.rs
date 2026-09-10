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

    let list_tool = ManageWorkspaceTool {
        operation: "list".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let list_result = list_tool.call_tool(&handler).await.unwrap();
    let list_text = extract_text_from_result(&list_result);
    assert!(
        list_text.contains(&format!("({}) [CURRENT]", target_id)),
        "list should use session primary binding, not stale workspace_id: {list_text}"
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
        open_text.contains("Workspace Missing But Still Active"),
        "open should block cleanup for a rebound current workspace while it is live: {open_text}"
    );
    assert!(
        open_text.contains("workspace is active in this in-process session"),
        "open should explain the in-process liveness block: {open_text}"
    );
    assert!(
        !open_text.contains("Workspace Refresh Failed"),
        "open should not fall back to the old refresh failure text: {open_text}"
    );
}
