use super::*;

#[tokio::test]
async fn test_unknown_workspace_suggests_closest_match() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;
    let typo_workspace_id = format!("{}x", target_id);

    let result = FastSearchTool {
        query: "target_search_marker".to_string(),
        limit: 10,
        workspace: Some(typo_workspace_id.clone()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await;

    let error = result.expect_err("unknown workspace typo should be rejected");
    let message = error.to_string();
    assert!(
        message.contains(&format!("Workspace '{}' not found", typo_workspace_id)),
        "error should identify the unknown workspace: {message}"
    );
    assert!(
        message.contains(&format!("Did you mean '{}'", target_id)),
        "error should suggest the closest known workspace id: {message}"
    );
}

#[tokio::test]
async fn test_workspace_resolution_failure_unknown_without_suggestion_is_typed() {
    let (_temp_dir, handler, _target_id) = setup_known_reference_search_workspace().await;
    let missing_workspace_id = "definitely_missing_workspace";

    let error = resolve_workspace_filter(Some(missing_workspace_id), &handler)
        .await
        .expect_err("unknown workspace should be rejected");

    assert_workspace_resolution_failure(
        &error,
        WorkspaceResolutionFailureKind::UnknownWorkspace,
        "Workspace 'definitely_missing_workspace' not found. Use 'primary' or a valid workspace ID",
    );
}

#[tokio::test]
async fn test_workspace_resolution_failure_unknown_with_suggestion_is_typed() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;
    let typo_workspace_id = format!("{}x", target_id);

    let error = resolve_workspace_filter(Some(&typo_workspace_id), &handler)
        .await
        .expect_err("unknown workspace typo should be rejected");

    assert_workspace_resolution_failure(
        &error,
        WorkspaceResolutionFailureKind::UnknownWorkspace,
        &format!(
            "Workspace '{}' not found. Did you mean '{}'?",
            typo_workspace_id, target_id
        ),
    );
}

#[tokio::test]
async fn test_workspace_resolution_failure_known_not_ready_is_typed() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;
    handler
        .daemon_db
        .as_ref()
        .expect("test handler should expose daemon db")
        .update_workspace_status(&target_id, "pending")
        .expect("workspace status should update for test");

    let error = resolve_workspace_filter(Some(&target_id), &handler)
        .await
        .expect_err("pending workspace should not auto-activate");

    assert_workspace_resolution_failure(
        &error,
        WorkspaceResolutionFailureKind::WorkspaceNotReady,
        &format!(
            "Workspace '{}' is known but has status 'pending' (not ready). Run manage_workspace(operation=\"open\", workspace_id=\"{}\") first.",
            target_id, target_id
        ),
    );
}

#[tokio::test]
async fn test_workspace_resolution_failure_auto_activation_failed_is_typed() {
    let (temp_dir, handler, _target_id) = setup_known_reference_search_workspace().await;
    let missing_root = temp_dir.path().join("missing-auto-activation");
    let missing_path = missing_root.to_string_lossy().to_string();
    let missing_workspace_id = generate_workspace_id(&missing_path).unwrap();
    handler
        .daemon_db
        .as_ref()
        .expect("test handler should expose daemon db")
        .upsert_workspace(&missing_workspace_id, &missing_path, "ready")
        .expect("missing workspace row should be registered for test");

    let error = resolve_workspace_filter(Some(&missing_workspace_id), &handler)
        .await
        .expect_err("missing workspace root should fail auto-activation");

    assert_workspace_resolution_failure(
        &error,
        WorkspaceResolutionFailureKind::AutoActivationFailed,
        &format!(
            "Workspace '{}' is known but auto-activation failed: Workspace path does not exist: {}. Run manage_workspace(operation=\"open\", workspace_id=\"{}\") first.",
            missing_workspace_id,
            missing_root.display(),
            missing_workspace_id
        ),
    );
}

#[tokio::test]
async fn test_deep_dive_invalid_workspace_uses_invalid_params_from_typed_resolution_failure() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;
    let typo_workspace_id = format!("{}x", target_id);

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("deep_dive").with_arguments(
            serde_json::json!({
                "symbol": "target_search_marker",
                "workspace": typo_workspace_id,
            })
            .as_object()
            .expect("deep_dive args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(12), service.peer().clone()),
    )
    .await;

    let error = result.expect_err("deep_dive should reject invalid workspace target");
    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    assert_eq!(
        error.message.as_ref(),
        format!(
            "deep_dive failed: Workspace '{}x' not found. Did you mean '{}'?",
            target_id, target_id
        )
    );

    let _ = service.cancel().await;
}
