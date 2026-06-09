use super::*;

// Finding #27: list/remove should not hard-fail with the misleading "run
// index" error when no primary is bound (e.g., a deferred Cwd session before
// the client has advertised roots). `register` now works without a primary,
// while legacy `add` should fail fast as an unknown operation.

fn make_deferred_handler_no_primary(
    indexes_dir: &std::path::Path,
    daemon_db: Arc<DaemonDatabase>,
    startup_root: &std::path::Path,
) -> impl std::future::Future<Output = JulieServerHandler> {
    let _ = indexes_dir;
    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    async move {
        let startup_ws = Arc::new(
            crate::workspace::JulieWorkspace::initialize(startup_path.clone())
                .await
                .expect("startup workspace should initialize"),
        );
        let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
            startup_ws,
            crate::workspace::startup_hint::WorkspaceStartupHint {
                path: startup_path,
                source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
            },
            Some(daemon_db),
            Some(startup_id),
            None,
            None,
        )
        .await
        .expect("handler should initialize");
        assert_eq!(
            handler.current_workspace_id(),
            None,
            "deferred Cwd session should leave current_workspace_id unset"
        );
        handler
    }
}

#[tokio::test]
async fn test_manage_workspace_list_succeeds_in_deferred_session_without_primary() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();
    let startup_root = temp_dir.path().join("startup");
    fs::create_dir_all(&startup_root).unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    // Pre-register two workspaces so list has something to show.
    let registered_path = temp_dir.path().join("registered").canonicalize().ok();
    let registered_id = registered_path
        .as_ref()
        .map(|p| generate_workspace_id(&p.to_string_lossy()).unwrap());
    if let (Some(path), Some(id)) = (registered_path.as_ref(), registered_id.as_ref()) {
        daemon_db
            .upsert_workspace(id, &path.to_string_lossy(), "ready")
            .unwrap();
    } else {
        // Use the startup root for the second workspace if `registered` isn't a real path.
        let startup_path = startup_root.canonicalize().unwrap();
        let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
        daemon_db
            .upsert_workspace(&startup_id, &startup_path.to_string_lossy(), "ready")
            .unwrap();
    }

    let handler =
        make_deferred_handler_no_primary(&indexes_dir, Arc::clone(&daemon_db), &startup_root).await;

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
    .expect("list should succeed in a deferred session without primary");

    let text = extract_text_from_result(&result);
    assert!(
        !text.contains("Run manage_workspace(operation=\"index\")"),
        "list should not direct the user to run index in a deferred session: {text}"
    );
    // Should either be an empty-list message or include workspace metadata —
    // both are valid; what matters is that we don't short-circuit with the
    // wrong error.
    assert!(
        text.contains("Registered Workspaces") || text.contains("No workspaces registered"),
        "list should produce a real listing, got: {text}"
    );
}

#[tokio::test]
async fn test_manage_workspace_remove_succeeds_in_deferred_session_without_primary() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();
    let startup_root = temp_dir.path().join("startup");
    let removable_root = temp_dir.path().join("removable");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&removable_root).unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let removable_path = removable_root.canonicalize().unwrap();
    let removable_id = generate_workspace_id(&removable_path.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&removable_id, &removable_path.to_string_lossy(), "ready")
        .unwrap();

    let handler =
        make_deferred_handler_no_primary(&indexes_dir, Arc::clone(&daemon_db), &startup_root).await;

    let result = ManageWorkspaceTool {
        operation: "remove".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(removable_id.clone()),
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("remove should succeed in a deferred session without primary");

    let text = extract_text_from_result(&result);
    assert!(
        !text.contains("Run manage_workspace(operation=\"index\")"),
        "remove should not direct the user to run index in a deferred session: {text}"
    );
    assert!(
        text.contains("Workspace Removed Successfully"),
        "remove should report success even without a bound primary, got: {text}"
    );
    assert!(
        daemon_db.get_workspace(&removable_id).unwrap().is_none(),
        "removable workspace should be gone from the daemon database"
    );
}

#[tokio::test]
async fn test_manage_workspace_register_succeeds_in_deferred_session_without_primary_and_keeps_workspace_inactive()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();
    let startup_root = temp_dir.path().join("startup");
    let candidate_root = temp_dir.path().join("candidate");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(candidate_root.join("src")).unwrap();
    fs::write(
        candidate_root.join("src/lib.rs"),
        "pub fn candidate_marker() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let handler =
        make_deferred_handler_no_primary(&indexes_dir, Arc::clone(&daemon_db), &startup_root).await;

    let candidate_path = candidate_root.canonicalize().unwrap();
    let candidate_path_str = candidate_path.to_string_lossy().to_string();
    let candidate_id = generate_workspace_id(&candidate_path_str).unwrap();

    let result = ManageWorkspaceTool {
        operation: "register".to_string(),
        path: Some(candidate_path_str),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("register should succeed without a bound primary");

    let text = extract_text_from_result(&result);
    assert!(
        text.contains(&candidate_id),
        "register output should include the workspace id: {text}"
    );
    assert!(
        daemon_db.get_workspace(&candidate_id).unwrap().is_some(),
        "register should persist the workspace row"
    );
    assert!(
        !handler.is_workspace_active(&candidate_id).await,
        "register should not activate the workspace for the session"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "register should not bind a deferred session to a primary workspace"
    );
}

#[tokio::test]
async fn test_manage_workspace_add_is_rejected_as_unknown_operation() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();
    let startup_root = temp_dir.path().join("startup");
    let candidate_root = temp_dir.path().join("candidate");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&candidate_root).unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let handler =
        make_deferred_handler_no_primary(&indexes_dir, Arc::clone(&daemon_db), &startup_root).await;

    let candidate_path_str = candidate_root
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let result = ManageWorkspaceTool {
        operation: "add".to_string(),
        path: Some(candidate_path_str),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await;

    let err = result.expect_err("add should be rejected");
    let message = err.to_string();
    assert!(
        message.contains("Unknown operation"),
        "add should fail as an unknown operation: {message}"
    );
    assert!(
        message.contains("register"),
        "add error should point at the supported command surface: {message}"
    );
}

#[tokio::test]
async fn test_manage_workspace_register_in_deferred_cwd_session_via_server_handler_keeps_primary_unbound()
 {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = temp_dir.path().join("startup-cwd");
    fs::create_dir_all(startup_root.join("src")).unwrap();
    fs::write(startup_root.join("src/lib.rs"), "pub fn cwd_marker() {}\n").unwrap();

    let candidate_root = temp_dir.path().join("reference-candidate");
    fs::create_dir_all(candidate_root.join("src")).unwrap();
    fs::write(
        candidate_root.join("src/lib.rs"),
        "pub fn candidate_marker() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_path_str = startup_path.to_string_lossy().to_string();
    let startup_id = generate_workspace_id(&startup_path_str).unwrap();
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
    )
    .await
    .expect("handler should initialize");

    // Client did NOT declare roots support. Under the old classification this
    // still triggered the primary-binding fallback in the preflight.
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "cwd startup hint should start without a bound current primary"
    );

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let candidate_path_str = candidate_root
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({
                "operation": "register",
                "path": candidate_path_str,
            })
            .as_object()
            .expect("manage_workspace register args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(1), service.peer().clone()),
    )
    .await
    .expect("register via ServerHandler should succeed");

    let message = extract_text_from_result(&result);
    let candidate_id = generate_workspace_id(&candidate_path_str).unwrap();
    assert!(
        message.contains(&candidate_id),
        "register output should include the workspace id: {message}"
    );

    assert_eq!(
        handler.current_workspace_id(),
        None,
        "register via deferred Cwd session must not silently bind the startup-hint as primary"
    );
    assert_eq!(
        handler.loaded_workspace_id(),
        Some(startup_id),
        "the loaded-workspace handle stays on the startup workspace; nothing should have swapped primary binding"
    );

    let row = daemon_db
        .get_workspace(&candidate_id)
        .unwrap()
        .expect("registered workspace row should exist");
    assert_eq!(row.path, candidate_path_str);
    assert_eq!(row.status, "ready");
}
