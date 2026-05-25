use super::*;

#[tokio::test]
async fn test_known_ready_workspace_auto_activates_for_fast_search() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;

    assert!(
        !handler.is_workspace_active(&target_id).await,
        "fresh session should start with an inactive target workspace"
    );

    let result = FastSearchTool {
        query: "target_search_marker".to_string(),
        limit: 10,
        workspace: Some(target_id.clone()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await
    .expect("known ready workspace should auto-activate on first query");

    let message = extract_text_from_result(&result);
    assert!(
        message.contains("target_search_marker"),
        "search should return data from the target workspace: {message}"
    );
    assert!(
        handler.is_workspace_active(&target_id).await,
        "target workspace should be active after first targeted query"
    );
}

#[tokio::test]
async fn test_known_pending_workspace_requires_open_before_fast_search() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;
    let daemon_db = handler
        .daemon_db
        .as_ref()
        .expect("test handler should expose daemon db")
        .clone();
    daemon_db
        .update_workspace_status(&target_id, "pending")
        .expect("workspace status should update for test");

    let result = FastSearchTool {
        query: "target_search_marker".to_string(),
        limit: 10,
        workspace: Some(target_id.clone()),
        ..Default::default()
    }
    .call_tool(&handler)
    .await;

    let error = result.expect_err("pending workspace should not auto-activate");
    let message = error.to_string();
    assert!(
        message.contains("status 'pending'"),
        "error should include workspace status: {message}"
    );
    assert!(
        message.contains("manage_workspace(operation=\"open\", workspace_id=\"")
            && message.contains(&target_id),
        "error should explain how to open the workspace first: {message}"
    );
}

#[tokio::test]
async fn test_known_workspace_row_does_not_preactivate_on_new_session() {
    let (temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;
    let daemon_db = handler
        .daemon_db
        .as_ref()
        .expect("test handler should expose daemon db")
        .clone();
    let primary_id = handler
        .loaded_workspace_id()
        .expect("test handler should expose primary workspace id");

    let primary_root = temp_dir.path().join("primary").canonicalize().unwrap();
    let primary_ws = handler
        .workspace_pool
        .as_ref()
        .expect("test handler should expose workspace pool")
        .get_or_init(&primary_id, primary_root.clone())
        .await
        .expect("primary workspace should still be loadable");

    let fresh_handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_root,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
        None,
        None,
        None,
        handler.workspace_pool.clone(),
    )
    .await
    .expect("fresh handler should initialize");

    assert!(
        !fresh_handler.is_workspace_active(&target_id).await,
        "known workspace rows should not pre-activate on a new session"
    );

    let result = FastSearchTool {
        query: "target_search_marker".to_string(),
        limit: 10,
        workspace: Some(target_id.clone()),
        ..Default::default()
    }
    .call_tool(&fresh_handler)
    .await
    .expect("first query should lazily activate the known workspace");

    let message = extract_text_from_result(&result);
    assert!(
        message.contains("target_search_marker"),
        "search should return data from the known workspace after lazy activation: {message}"
    );
    assert!(
        fresh_handler.is_workspace_active(&target_id).await,
        "known workspace should become active after first targeted query"
    );
}

#[tokio::test]
async fn test_opened_workspace_routes_fast_search_by_workspace_id() {
    let (_temp_dir, handler, target_id) = setup_known_reference_search_workspace().await;

    let open_result = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(target_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("open should succeed for known workspace");
    let open_text = extract_text_from_result(&open_result);
    assert!(
        open_text.contains("Workspace Opened"),
        "open should confirm success: {open_text}"
    );
    assert!(handler.is_workspace_active(&target_id).await);

    let result = FastSearchTool {
        query: "target_search_marker".to_string(),
        limit: 10,
        workspace: Some(target_id),
        ..Default::default()
    }
    .call_tool(&handler)
    .await
    .expect("opened workspace should be searchable");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains("target_search_marker"),
        "search should route to the opened target workspace: {text}"
    );
}
