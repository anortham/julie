use super::*;

#[tokio::test]
async fn test_secondary_scoped_request_does_not_consume_dirty_roots() -> Result<()> {
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
    ));
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace = pool
        .get_or_init(&startup_workspace_id, startup_path.clone())
        .await?;

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
        None,
        Some(Arc::clone(&pool)),
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
        RequestContext::new(NumberOrString::Number(13), service.peer().clone()),
    );
    let (initial_roots_result, initial_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(initial_roots_reply, initial_list_future)
        })
        .await
        .expect("initial roots binding should finish");
    initial_roots_result?;
    initial_list_result?;

    let root_b_id = crate::workspace::registry::generate_workspace_id(
        &root_b.path().canonicalize()?.to_string_lossy(),
    )?;
    let secondary_id = crate::workspace::registry::generate_workspace_id(
        &secondary_root.path().canonicalize()?.to_string_lossy(),
    )?;

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/roots/list_changed"
        }),
    )
    .await?;
    tokio::task::yield_now().await;

    let secondary_search = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("fast_search").with_arguments(
            serde_json::json!({
                "query": "secondary_root",
                "workspace": secondary_id,
                "limit": 5
            })
            .as_object()
            .expect("fast_search args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(14), service.peer().clone()),
    );
    let secondary_result = secondary_search.await?;
    let secondary_text = extract_text(&secondary_result);
    assert!(
        secondary_text.contains("secondary_root"),
        "secondary-scoped request should keep working while roots are dirty; got: {secondary_text}"
    );
    assert!(
        handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty(),
        "secondary-scoped requests must not consume the dirty bit"
    );

    let refreshed_roots_paths = [root_b.path(), secondary_root.path()];
    let refreshed_roots_reply =
        answer_roots_request(&mut lines, &mut write_half, &refreshed_roots_paths);
    let primary_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(15), service.peer().clone()),
    );
    let (refreshed_roots_result, primary_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(refreshed_roots_reply, primary_list_future)
        })
        .await
        .expect("primary-scoped request should consume dirty roots");
    refreshed_roots_result?;
    primary_list_result?;

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
        "the next primary-scoped request should consume the dirty bit"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_fast_search_reference_definitions_first_request_succeeds_without_primary_bind()
-> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let roots_root = tempfile::tempdir()?;
    let secondary_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join(".julie"))?;
    std::fs::create_dir_all(roots_root.path().join("src"))?;
    std::fs::create_dir_all(secondary_root.path().join("src"))?;
    std::fs::write(
        roots_root.path().join("src/lib.rs"),
        "pub fn roots_root() {}\n",
    )?;
    std::fs::write(
        secondary_root.path().join("src/lib.rs"),
        "pub fn secondary_first_request_symbol() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
    ));
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace = pool
        .get_or_init(&startup_workspace_id, startup_path.clone())
        .await?;

    let prep_handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_workspace,
        WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_workspace_id.clone()),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&restart_pending)),
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    let secondary_path = secondary_root.path().canonicalize()?;
    let secondary_id =
        crate::workspace::registry::generate_workspace_id(&secondary_path.to_string_lossy())?;
    let index_result = crate::tools::workspace::ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(secondary_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&prep_handler)
    .await?;
    let index_text = extract_text(&index_result);
    assert!(
        index_text.contains("Workspace indexing complete")
            || index_text.contains("Workspace already indexed"),
        "secondary indexing should complete during setup: {index_text}"
    );

    let startup_workspace = pool
        .get_or_init(&startup_workspace_id, startup_path.clone())
        .await?;
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
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let lines = BufReader::new(read_half).lines();

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
        "reference-scoped first request should start before any primary bind"
    );
    assert!(
        !handler.is_workspace_active(&secondary_id).await,
        "workspace-scoped request should activate the target through the normal resolver path"
    );

    let search_result = tokio::time::timeout(Duration::from_secs(10), async {
        <JulieServerHandler as ServerHandler>::call_tool(
            &handler,
            CallToolRequestParams::new("fast_search").with_arguments(
                serde_json::json!({
                    "query": "secondary_first_request_symbol",
                    "workspace": secondary_id,
                    "limit": 5,
                })
                .as_object()
                .expect("fast_search args")
                .clone(),
            ),
            RequestContext::new(NumberOrString::Number(30), service.peer().clone()),
        )
        .await
    })
    .await
    .expect("reference definition search should not block on primary binding")?;

    let search_text = extract_text(&search_result);
    assert!(
        search_text.contains("secondary_first_request_symbol"),
        "reference definition search should return the indexed symbol before any primary bind: {search_text}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "reference-scoped definition search must not bind the primary workspace"
    );
    assert!(
        handler.is_workspace_active(&secondary_id).await,
        "workspace-scoped request should auto-activate the target workspace"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_fast_search_startup_workspace_id_first_request_succeeds_without_primary_bind()
-> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn startup_first_request_symbol() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
    ));
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace = pool
        .get_or_init(&startup_workspace_id, startup_path.clone())
        .await?;

    let prep_handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_workspace,
        WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_workspace_id.clone()),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&restart_pending)),
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;
    assert_eq!(
        prep_handler.current_workspace_id().as_deref(),
        Some(startup_workspace_id.as_str()),
        "setup handler should bind the startup workspace as primary"
    );

    let index_result = crate::tools::workspace::ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&prep_handler)
    .await?;
    let index_text = extract_text(&index_result);
    assert!(
        index_text.contains("Workspace indexing complete")
            || index_text.contains("Workspace already indexed"),
        "startup indexing should complete during setup: {index_text}"
    );

    let startup_workspace = pool
        .get_or_init(&startup_workspace_id, startup_path.clone())
        .await?;
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
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (_read_half, _write_half) = tokio::io::split(client_transport);

    assert_eq!(
        handler.current_workspace_id(),
        None,
        "startup-scoped first request should start before any primary bind"
    );
    assert!(
        handler
            .was_workspace_attached_in_session(&startup_workspace_id)
            .await
    );
    assert!(!handler.is_workspace_active(&startup_workspace_id).await);

    let search_result = tokio::time::timeout(Duration::from_secs(10), async {
        <JulieServerHandler as ServerHandler>::call_tool(
            &handler,
            CallToolRequestParams::new("fast_search").with_arguments(
                serde_json::json!({
                    "query": "startup_first_request_symbol",
                    "workspace": startup_workspace_id,
                    "limit": 5,
                })
                .as_object()
                .expect("fast_search args")
                .clone(),
            ),
            RequestContext::new(NumberOrString::Number(31), service.peer().clone()),
        )
        .await
    })
    .await
    .expect("startup definition search should not block on primary binding")?;

    let search_text = extract_text(&search_result);
    assert!(
        search_text.contains("startup_first_request_symbol"),
        "startup definition search should return the indexed symbol before any primary bind: {search_text}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "startup-scoped definition search must not bind the primary workspace"
    );

    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_manage_workspace_stats_secondary_target_does_not_consume_dirty_roots() -> Result<()> {
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
    ));
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace = pool
        .get_or_init(&startup_workspace_id, startup_path.clone())
        .await?;

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
        None,
        Some(Arc::clone(&pool)),
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
        RequestContext::new(NumberOrString::Number(22), service.peer().clone()),
    );
    let (initial_roots_result, initial_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(initial_roots_reply, initial_list_future)
        })
        .await
        .expect("initial roots binding should finish");
    initial_roots_result?;
    initial_list_result?;

    let root_b_id = crate::workspace::registry::generate_workspace_id(
        &root_b.path().canonicalize()?.to_string_lossy(),
    )?;
    let secondary_id = crate::workspace::registry::generate_workspace_id(
        &secondary_root.path().canonicalize()?.to_string_lossy(),
    )?;

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/roots/list_changed"
        }),
    )
    .await?;
    tokio::task::yield_now().await;

    let secondary_stats_result = tokio::time::timeout(
        Duration::from_secs(2),
        <JulieServerHandler as ServerHandler>::call_tool(
            &handler,
            CallToolRequestParams::new("manage_workspace").with_arguments(
                serde_json::json!({
                    "operation": "stats",
                    "workspace_id": secondary_id
                })
                .as_object()
                .expect("manage_workspace stats args")
                .clone(),
            ),
            RequestContext::new(NumberOrString::Number(23), service.peer().clone()),
        ),
    )
    .await
    .expect("secondary-targeted stats should not wait on roots/list");
    secondary_stats_result?;

    assert!(
        handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty(),
        "secondary-targeted stats must not consume roots_dirty"
    );

    let refreshed_roots_paths = [root_b.path(), secondary_root.path()];
    let refreshed_roots_reply =
        answer_roots_request(&mut lines, &mut write_half, &refreshed_roots_paths);
    let primary_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(24), service.peer().clone()),
    );
    let (refreshed_roots_result, primary_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(refreshed_roots_reply, primary_list_future)
        })
        .await
        .expect("primary-scoped request should still consume dirty roots");
    refreshed_roots_result?;
    primary_list_result?;

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
        "the next primary-scoped request should still consume the dirty bit"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_roots_list_changed_unbound_empty_refresh_settles_dirty_state() -> Result<()> {
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
    ));
    let restart_pending = Arc::new(AtomicBool::new(false));

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_workspace = pool
        .get_or_init(&startup_workspace_id, startup_path.clone())
        .await?;

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
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    assert_eq!(handler.current_workspace_id(), None);

    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

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
        "roots/list_changed should dirty an unbound session too"
    );

    let empty_roots_paths: [&Path; 0] = [];
    let empty_roots_reply = answer_roots_request(&mut lines, &mut write_half, &empty_roots_paths);
    let first_list_future = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({ "operation": "list" })
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(16), service.peer().clone()),
    );
    let (empty_roots_result, first_list_result) =
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(empty_roots_reply, first_list_future)
        })
        .await
        .expect("unbound empty roots refresh should finish");
    empty_roots_result?;
    first_list_result?;

    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(startup_workspace_id.as_str())
    );
    assert_eq!(handler.current_workspace_root(), startup_path);
    assert!(
        !handler
            .session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .roots_dirty(),
        "unbound empty roots refresh should settle dirty state"
    );

    let second_list_result = tokio::time::timeout(
        Duration::from_secs(2),
        <JulieServerHandler as ServerHandler>::call_tool(
            &handler,
            CallToolRequestParams::new("manage_workspace").with_arguments(
                serde_json::json!({ "operation": "list" })
                    .as_object()
                    .expect("manage_workspace list args")
                    .clone(),
            ),
            RequestContext::new(NumberOrString::Number(17), service.peer().clone()),
        ),
    )
    .await
    .expect("settled primary-scoped request should not wait on another roots/list");
    second_list_result?;

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}
