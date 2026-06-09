use super::*;

#[tokio::test]
async fn test_initialized_weak_cwd_eagerly_probes_and_binds_roots() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let roots_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::create_dir_all(roots_root.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn from_startup() {}\n",
    )?;
    std::fs::write(
        roots_root.path().join("src/lib.rs"),
        "pub fn from_roots() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

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
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )
    .await?;

    let roots_paths = [roots_root.path()];
    tokio::time::timeout(
        Duration::from_secs(10),
        answer_roots_request(&mut lines, &mut write_half, &roots_paths),
    )
    .await
    .expect("on_initialized with cwd startup should eagerly send roots/list")?;

    let roots_workspace_id = crate::workspace::registry::generate_workspace_id(
        &roots_root.path().canonicalize()?.to_string_lossy(),
    )?;

    let bound = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if handler.current_workspace_id().as_deref() == Some(roots_workspace_id.as_str()) {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(
        bound.is_ok(),
        "on_initialized should eagerly bind the roots workspace without a tool call"
    );
    assert_eq!(
        handler.current_workspace_root(),
        roots_root.path().canonicalize()?
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}
#[tokio::test]
#[ignore = "daemon multi-workspace session/roots lifecycle (pool-backed); reworked in Phase 3d.3 registry rework"]
async fn test_manage_workspace_index_uses_roots_over_cwd_hint() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let roots_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join(".julie"))?;
    std::fs::create_dir_all(roots_root.path().join("src"))?;
    std::fs::write(
        roots_root.path().join("src/lib.rs"),
        "pub fn from_roots() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

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
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )
    .await?;

    assert_eq!(
        handler.current_workspace_id(),
        None,
        "cwd-derived startup should not pre-bind the startup workspace as current primary"
    );

    let roots_paths = [roots_root.path()];
    let roots_reply = answer_roots_request(&mut lines, &mut write_half, &roots_paths);
    let list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({
                "operation": "list"
            })
            .as_object()
            .expect("manage_workspace list args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(2), service.peer().clone()),
    );
    let (roots_result, list_result) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(roots_reply, list_future)
    })
    .await
    .expect("manage_workspace list should finish after roots/list resolution");
    roots_result?;
    let result = list_result?;

    let roots_workspace_id = crate::workspace::registry::generate_workspace_id(
        &roots_root.path().canonicalize()?.to_string_lossy(),
    )?;
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(roots_workspace_id.as_str()),
        "the first primary-scoped request should bind the current primary to the client roots workspace"
    );
    assert!(
        handler.current_workspace_root() == roots_root.path().canonicalize()?,
        "the wrapped manage_workspace list call should rebind the current primary root from cwd to the client roots workspace"
    );
    let text = extract_text(&result);
    assert!(
        text.contains(&roots_workspace_id),
        "manage_workspace list should report the roots-bound current primary: {text}"
    );

    let indexed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(db) = handler
                .get_database_for_workspace(&roots_workspace_id)
                .await
            {
                let count = db
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .count_symbols_for_workspace()?;
                if count > 0 {
                    break Ok::<bool, anyhow::Error>(true);
                }
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("deferred auto-index should resume after first primary bind")?;
    assert!(
        indexed,
        "first successful primary bind should resume deferred auto-indexing"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_roots_attach_failure_does_not_leave_stuck_primary_binding() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let roots_root = tempfile::tempdir()?;
    let bad_roots_file = startup_root.path().join("not-a-workspace.txt");
    std::fs::write(&bad_roots_file, "bad")?;
    std::fs::create_dir_all(roots_root.path().join("src"))?;
    std::fs::write(
        roots_root.path().join("src/lib.rs"),
        "pub fn from_roots() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

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
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let bad_roots_paths = [bad_roots_file.as_path()];
    let bad_roots_reply = answer_roots_request(&mut lines, &mut write_half, &bad_roots_paths);
    let first_call = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(3), service.peer().clone()),
    );
    let (bad_roots_result, first_result) = tokio::join!(bad_roots_reply, first_call);
    bad_roots_result?;
    let first_error = first_result.expect_err("bad roots attachment should fail");
    assert!(
        first_error.to_string().contains("workspace")
            || first_error.to_string().contains("initialize"),
        "unexpected first error: {first_error:#}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "failed roots attachment must not leave a bound current primary"
    );

    let good_roots_paths = [roots_root.path()];
    let good_roots_reply = answer_roots_request(&mut lines, &mut write_half, &good_roots_paths);
    let second_call = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(4), service.peer().clone()),
    );
    let (good_roots_result, second_result) = tokio::join!(good_roots_reply, second_call);
    good_roots_result?;
    let second_result = second_result?;
    let roots_workspace_id = crate::workspace::registry::generate_workspace_id(
        &roots_root.path().canonicalize()?.to_string_lossy(),
    )?;
    assert_eq!(
        handler.current_workspace_id(),
        Some(roots_workspace_id.clone())
    );
    assert!(
        extract_text(&second_result).contains(&roots_workspace_id),
        "successful retry should bind and report the roots workspace"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_first_request_roots_failure_falls_back_to_startup_hint_without_initialized_snapshot()
-> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let roots_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join(".julie"))?;
    std::fs::create_dir_all(roots_root.path().join("src"))?;
    std::fs::write(
        roots_root.path().join("src/lib.rs"),
        "pub fn from_roots() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

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
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )
    .await?;

    assert_eq!(
        handler.current_workspace_id(),
        None,
        "weak cwd startup should stay unbound until a primary-scoped request resolves roots"
    );

    let first_roots_reply =
        answer_roots_request_error(&mut lines, &mut write_half, "transient roots failure");
    let first_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(30), service.peer().clone()),
    );
    let (first_roots_result, first_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(first_roots_reply, first_list_future)
        })
        .await
        .expect("first primary-scoped request should finish after transient roots failure");
    first_roots_result?;
    let first_result = first_list_result?;

    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(startup_workspace_id.as_str()),
        "without an initialized roots snapshot, a transient request-time roots failure should fall back to startup cwd"
    );
    assert_eq!(handler.current_workspace_root(), startup_path);
    assert!(
        extract_text(&first_result).contains(&startup_workspace_id),
        "the primary-scoped request should fall back to the startup workspace when request-time roots lookup fails"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
#[ignore = "daemon multi-workspace session/roots lifecycle (pool-backed); reworked in Phase 3d.3 registry rework"]
async fn test_same_root_request_time_attach_does_not_leak_session_count() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn startup() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

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
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    wait_for_session_count(&daemon_db, &startup_workspace_id, 1).await;

    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();
    let roots_paths = [startup_root.path()];
    let roots_reply = answer_roots_request(&mut lines, &mut write_half, &roots_paths);
    let list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(5), service.peer().clone()),
    );
    let (roots_result, list_result) = tokio::join!(roots_reply, list_future);
    roots_result?;
    let result = list_result?;
    let text = extract_text(&result);
    assert!(
        text.contains(&startup_workspace_id),
        "same-root roots resolution should still succeed: {text}"
    );
    wait_for_session_count(&daemon_db, &startup_workspace_id, 1).await;

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    handler
        .detach_workspace_for_session(&startup_workspace_id)
        .await?;
    wait_for_session_count(&daemon_db, &startup_workspace_id, 0).await;
    Ok(())
}
