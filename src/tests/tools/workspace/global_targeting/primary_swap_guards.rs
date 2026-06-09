use super::*;

/// Regression for Findings #28/#29: when a primary workspace swap is in progress,
/// `open` and `refresh` must refuse to mutate primary binding. Otherwise the
/// secondary-path `initialize_workspace_with_force` call can race the swap
/// machinery and clobber half-applied state.
async fn build_primary_bound_handler_for_swap_guard_test()
-> (tempfile::TempDir, JulieServerHandler, String, String) {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let primary_root = temp_dir.path().join("primary");
    let target_root = temp_dir.path().join("target");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&target_root).unwrap();
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
    daemon_db
        .upsert_workspace(&primary_id, &primary_path_str, "ready")
        .unwrap();

    let target_path = target_root.canonicalize().unwrap();
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_id = generate_workspace_id(&target_path_str).unwrap();
    daemon_db
        .upsert_workspace(&target_id, &target_path_str, "ready")
        .unwrap();

    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(primary_id.clone()),
        None,
        None,
    )
    .await
    .expect("handler should initialize");

    (temp_dir, handler, primary_id, target_id)
}

#[tokio::test]
async fn test_workspace_resolution_failure_primary_swap_in_progress_is_typed() {
    let (_temp_dir, handler, _primary_id, target_id) =
        build_primary_bound_handler_for_swap_guard_test().await;
    handler
        .session_workspace
        .write()
        .unwrap()
        .begin_primary_swap();

    let error = resolve_workspace_filter(Some(&target_id), &handler)
        .await
        .expect_err("workspace-scoped query should wait for primary swap");

    assert_workspace_resolution_failure(
        &error,
        WorkspaceResolutionFailureKind::PrimarySwapInProgress,
        "Primary workspace swap in progress; retry workspace-scoped query after the swap completes.",
    );
}

#[tokio::test]
async fn test_manage_workspace_open_refuses_while_primary_swap_in_progress() {
    let (_temp_dir, handler, _primary_id, target_id) =
        build_primary_bound_handler_for_swap_guard_test().await;

    // Simulate an in-flight primary workspace swap.
    handler
        .session_workspace
        .write()
        .unwrap()
        .begin_primary_swap();
    assert!(handler.is_primary_workspace_swap_in_progress());

    let result = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await;

    let err = result.expect_err("open must refuse to run while a primary swap is in progress");
    let message = err.to_string().to_lowercase();
    assert!(
        message.contains("swap") && (message.contains("progress") || message.contains("retry")),
        "error should name the in-flight swap and suggest retry: {message}"
    );
}

#[tokio::test]
async fn test_manage_workspace_refresh_refuses_primary_mutation_while_swap_in_progress() {
    let (_temp_dir, handler, primary_id, _target_id) =
        build_primary_bound_handler_for_swap_guard_test().await;

    // Simulate an in-flight primary workspace swap.
    handler
        .session_workspace
        .write()
        .unwrap()
        .begin_primary_swap();
    assert!(handler.is_primary_workspace_swap_in_progress());

    // Refresh targeting the current primary WITH force=true goes through the
    // handle_index_command path, which mutates primary binding. Must refuse.
    let result = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(primary_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await;

    let err = result
        .expect_err("refresh must refuse to mutate primary binding while a swap is in progress");
    let message = err.to_string().to_lowercase();
    assert!(
        message.contains("swap") && (message.contains("progress") || message.contains("retry")),
        "error should name the in-flight swap and suggest retry: {message}"
    );
}
