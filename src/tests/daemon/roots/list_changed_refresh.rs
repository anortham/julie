use super::*;

#[tokio::test]
async fn test_roots_list_changed_marks_session_dirty_until_next_request() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let root_a = tempfile::tempdir()?;
    let root_b = tempfile::tempdir()?;
    let secondary_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join(".julie"))?;
    std::fs::create_dir_all(root_a.path().join("src"))?;
    std::fs::create_dir_all(root_b.path().join("src"))?;
    std::fs::create_dir_all(secondary_root.path().join("src"))?;
    std::fs::write(root_a.path().join("src/lib.rs"), "pub fn root_a() {}\n")?;
    std::fs::write(root_b.path().join("src/lib.rs"), "pub fn root_b() {}\n")?;
    std::fs::write(
        secondary_root.path().join("src/lib.rs"),
        "pub fn secondary_root() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace =
        Arc::new(crate::workspace::JulieWorkspace::initialize(startup_path.clone()).await?);

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_workspace,
        WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_workspace_id.clone()),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&restart_pending)),
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let initial_roots_paths = [root_a.path(), secondary_root.path()];
    let initial_roots_reply =
        answer_roots_request(&mut lines, &mut write_half, &initial_roots_paths);
    let first_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(6), service.peer().clone()),
    );
    let (initial_roots_result, first_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(initial_roots_reply, first_list_future)
        })
        .await
        .expect("first manage_workspace list should finish after roots/list resolution");
    initial_roots_result?;
    first_list_result?;

    let root_a_id = crate::workspace::registry::generate_workspace_id(
        &root_a.path().canonicalize()?.to_string_lossy(),
    )?;
    let root_b_id = crate::workspace::registry::generate_workspace_id(
        &root_b.path().canonicalize()?.to_string_lossy(),
    )?;
    let secondary_id = crate::workspace::registry::generate_workspace_id(
        &secondary_root.path().canonicalize()?.to_string_lossy(),
    )?;

    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(root_a_id.as_str())
    );
    assert!(
        handler.is_workspace_active(&secondary_id).await,
        "secondary roots should become active for explicit workspace targeting"
    );

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/roots/list_changed"
        }),
    )
    .await?;
    tokio::task::yield_now().await;

    assert!(
        handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty(),
        "roots/list_changed should mark the session dirty until the next primary-scoped request"
    );
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(root_a_id.as_str()),
        "roots/list_changed must not rebind the session in the middle of a tool call"
    );

    let refreshed_roots_paths = [root_b.path()];
    let refreshed_roots_reply =
        answer_roots_request(&mut lines, &mut write_half, &refreshed_roots_paths);
    let second_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(7), service.peer().clone()),
    );
    let (refreshed_roots_result, second_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(refreshed_roots_reply, second_list_future)
        })
        .await
        .expect("second manage_workspace list should finish after dirty roots refresh");
    refreshed_roots_result?;
    second_list_result?;

    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(root_b_id.as_str())
    );
    assert_eq!(
        handler.current_workspace_root(),
        root_b.path().canonicalize()?
    );
    assert!(
        handler.is_workspace_active(&secondary_id).await,
        "secondary roots should stay active for the session after the primary root changes"
    );
    assert!(
        !handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty(),
        "the next primary-scoped request should reconcile the dirty roots state"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_roots_list_changed_retries_after_failed_refresh() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let root_a = tempfile::tempdir()?;
    let root_b = tempfile::tempdir()?;
    let bad_root_file = startup_root.path().join("not-a-workspace.txt");
    std::fs::create_dir_all(startup_root.path().join(".julie"))?;
    std::fs::create_dir_all(root_a.path().join("src"))?;
    std::fs::create_dir_all(root_b.path().join("src"))?;
    std::fs::write(root_a.path().join("src/lib.rs"), "pub fn root_a() {}\n")?;
    std::fs::write(root_b.path().join("src/lib.rs"), "pub fn root_b() {}\n")?;
    std::fs::write(&bad_root_file, "bad\n")?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace =
        Arc::new(crate::workspace::JulieWorkspace::initialize(startup_path.clone()).await?);

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_workspace,
        WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_workspace_id.clone()),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&restart_pending)),
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let initial_roots_paths = [root_a.path()];
    let initial_roots_reply =
        answer_roots_request(&mut lines, &mut write_half, &initial_roots_paths);
    let initial_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(8), service.peer().clone()),
    );
    let (initial_roots_result, initial_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(initial_roots_reply, initial_list_future)
        })
        .await
        .expect("initial roots binding should finish");
    initial_roots_result?;
    initial_list_result?;

    let root_a_id = crate::workspace::registry::generate_workspace_id(
        &root_a.path().canonicalize()?.to_string_lossy(),
    )?;
    let root_b_id = crate::workspace::registry::generate_workspace_id(
        &root_b.path().canonicalize()?.to_string_lossy(),
    )?;
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(root_a_id.as_str())
    );

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/roots/list_changed"
        }),
    )
    .await?;
    tokio::task::yield_now().await;
    assert!(
        handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty()
    );

    let bad_roots_paths = [bad_root_file.as_path()];
    let bad_roots_reply = answer_roots_request(&mut lines, &mut write_half, &bad_roots_paths);
    let bad_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(9), service.peer().clone()),
    );
    let (bad_roots_result, bad_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(bad_roots_reply, bad_list_future)
        })
        .await
        .expect("bad roots refresh should finish");
    bad_roots_result?;
    let bad_error = bad_list_result.expect_err("bad roots refresh should fail");
    assert!(
        bad_error.to_string().contains("workspace") || bad_error.to_string().contains("initialize"),
        "unexpected bad roots error: {bad_error:#}"
    );
    assert!(
        handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty(),
        "failed dirty refresh must keep roots_dirty set so the next request retries"
    );
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(root_a_id.as_str())
    );

    let good_roots_paths = [root_b.path()];
    let good_roots_reply = answer_roots_request(&mut lines, &mut write_half, &good_roots_paths);
    let good_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(10), service.peer().clone()),
    );
    let (good_roots_result, good_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(good_roots_reply, good_list_future)
        })
        .await
        .expect("good roots retry should finish");
    good_roots_result?;
    good_list_result?;

    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(root_b_id.as_str())
    );
    assert!(
        !handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty(),
        "successful dirty refresh should clear roots_dirty"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_roots_list_changed_roots_list_failure_does_not_serve_stale_primary() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let root_a = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join(".julie"))?;
    std::fs::create_dir_all(root_a.path().join("src"))?;
    std::fs::write(root_a.path().join("src/lib.rs"), "pub fn root_a() {}\n")?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace =
        Arc::new(crate::workspace::JulieWorkspace::initialize(startup_path.clone()).await?);

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_workspace,
        WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_workspace_id.clone()),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&restart_pending)),
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let initial_roots_paths = [root_a.path()];
    let initial_roots_reply =
        answer_roots_request(&mut lines, &mut write_half, &initial_roots_paths);
    let initial_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(30), service.peer().clone()),
    );
    let (initial_roots_result, initial_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(initial_roots_reply, initial_list_future)
        })
        .await
        .expect("initial roots binding should finish");
    initial_roots_result?;
    initial_list_result?;

    let root_a_id = crate::workspace::registry::generate_workspace_id(
        &root_a.path().canonicalize()?.to_string_lossy(),
    )?;
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(root_a_id.as_str())
    );

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/roots/list_changed"
        }),
    )
    .await?;
    tokio::task::yield_now().await;
    assert!(
        handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty()
    );

    let failed_roots_reply =
        answer_roots_request_error(&mut lines, &mut write_half, "roots/list boom");
    let failed_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(31), service.peer().clone()),
    );
    let (failed_roots_result, failed_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(failed_roots_reply, failed_list_future)
        })
        .await
        .expect("roots/list failure refresh should finish");
    failed_roots_result?;
    let failed_error = failed_list_result.expect_err("roots/list failure should fail request");
    assert!(
        failed_error.to_string().contains("roots/list failed")
            || failed_error.to_string().contains("roots/list boom"),
        "unexpected roots/list failure error: {failed_error:#}"
    );
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(root_a_id.as_str()),
        "failed dirty refresh should not silently swap away from the last bound primary"
    );
    assert!(
        handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty(),
        "failed roots/list refresh must keep roots_dirty set so the next request retries"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_roots_list_changed_empty_refresh_reconciles_to_startup_hint() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let root_a = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::create_dir_all(root_a.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn startup() {}\n",
    )?;
    std::fs::write(root_a.path().join("src/lib.rs"), "pub fn root_a() {}\n")?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace =
        Arc::new(crate::workspace::JulieWorkspace::initialize(startup_path.clone()).await?);

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_workspace,
        WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_workspace_id.clone()),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&restart_pending)),
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let initial_roots_paths = [root_a.path()];
    let initial_roots_reply =
        answer_roots_request(&mut lines, &mut write_half, &initial_roots_paths);
    let initial_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(11), service.peer().clone()),
    );
    let (initial_roots_result, initial_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(initial_roots_reply, initial_list_future)
        })
        .await
        .expect("initial roots binding should finish");
    initial_roots_result?;
    initial_list_result?;

    let root_a_id = crate::workspace::registry::generate_workspace_id(
        &root_a.path().canonicalize()?.to_string_lossy(),
    )?;
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(root_a_id.as_str())
    );

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/roots/list_changed"
        }),
    )
    .await?;
    tokio::task::yield_now().await;

    let empty_roots_paths: [&Path; 0] = [];
    let empty_roots_reply = answer_roots_request(&mut lines, &mut write_half, &empty_roots_paths);
    let second_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(12), service.peer().clone()),
    );
    let (empty_roots_result, second_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(empty_roots_reply, second_list_future)
        })
        .await
        .expect("empty roots refresh should finish");
    empty_roots_result?;
    second_list_result?;

    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(startup_workspace_id.as_str()),
        "empty dirty refresh should settle back to the startup-hint primary"
    );
    assert_eq!(handler.current_workspace_root(), startup_path);
    assert!(
        !handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty(),
        "empty roots refresh should settle the dirty state instead of retrying forever"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_roots_list_changed_startup_hint_fallback_preserves_active_secondary() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let root_a = tempfile::tempdir()?;
    let secondary_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::create_dir_all(root_a.path().join("src"))?;
    std::fs::create_dir_all(secondary_root.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn startup() {}\n",
    )?;
    std::fs::write(root_a.path().join("src/lib.rs"), "pub fn root_a() {}\n")?;
    std::fs::write(
        secondary_root.path().join("src/lib.rs"),
        "pub fn secondary_root() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace =
        Arc::new(crate::workspace::JulieWorkspace::initialize(startup_path.clone()).await?);

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_workspace,
        WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_workspace_id.clone()),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&restart_pending)),
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let initial_roots_paths = [root_a.path(), secondary_root.path()];
    let initial_roots_reply =
        answer_roots_request(&mut lines, &mut write_half, &initial_roots_paths);
    let initial_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(20), service.peer().clone()),
    );
    let (initial_roots_result, initial_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(initial_roots_reply, initial_list_future)
        })
        .await
        .expect("initial roots binding should finish");
    initial_roots_result?;
    initial_list_result?;

    let secondary_id = crate::workspace::registry::generate_workspace_id(
        &secondary_root.path().canonicalize()?.to_string_lossy(),
    )?;
    assert!(handler.is_workspace_active(&secondary_id).await);

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/roots/list_changed"
        }),
    )
    .await?;
    tokio::task::yield_now().await;

    let empty_roots_paths: [&Path; 0] = [];
    let empty_roots_reply = answer_roots_request(&mut lines, &mut write_half, &empty_roots_paths);
    let fallback_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(21), service.peer().clone()),
    );
    let (empty_roots_result, fallback_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(empty_roots_reply, fallback_list_future)
        })
        .await
        .expect("startup-hint fallback should finish");
    empty_roots_result?;
    fallback_list_result?;

    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(startup_workspace_id.as_str())
    );
    assert!(
        handler.is_workspace_active(&secondary_id).await,
        "startup-hint fallback should preserve already-active secondary workspaces"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}
