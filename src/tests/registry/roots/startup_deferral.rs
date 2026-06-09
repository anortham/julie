use super::*;

#[tokio::test]
async fn test_roots_list_changed_explicit_startup_does_not_rebind() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let alternate_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::create_dir_all(alternate_root.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn startup() {}\n",
    )?;
    std::fs::write(
        alternate_root.path().join("src/lib.rs"),
        "pub fn alternate() {}\n",
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
            source: Some(WorkspaceStartupSource::Cli),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_workspace_id.clone()),
        Some(Arc::clone(&embedding_service)),
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(startup_workspace_id.as_str())
    );

    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (_read_half, mut write_half) = tokio::io::split(client_transport);

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
        "roots/list_changed should mark explicit-startup sessions dirty too"
    );

    let list_result = tokio::time::timeout(
        Duration::from_secs(2),
        <JulieServerHandler as ServerHandler>::call_tool(
            &handler,
            CallToolRequestParams::new("manage_workspace").with_arguments(
                serde_json::json!({ "operation": "list" })
                    .as_object()
                    .expect("manage_workspace list args")
                    .clone(),
            ),
            RequestContext::new(NumberOrString::Number(18), service.peer().clone()),
        ),
    )
    .await
    .expect("explicit-startup primary request should not wait on roots/list");
    list_result?;

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
        "explicit-startup primary request should settle dirty state without rebinding"
    );

    drop(write_half);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_roots_list_changed_env_startup_does_not_rebind() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let alternate_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::create_dir_all(alternate_root.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn startup() {}\n",
    )?;
    std::fs::write(
        alternate_root.path().join("src/lib.rs"),
        "pub fn alternate() {}\n",
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
            source: Some(WorkspaceStartupSource::Env),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_workspace_id.clone()),
        Some(Arc::clone(&embedding_service)),
        None,
    )
    .await?;

    handler.set_client_supports_workspace_roots_for_test(true);
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(startup_workspace_id.as_str())
    );

    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (_read_half, mut write_half) = tokio::io::split(client_transport);

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
        "roots/list_changed should mark env-startup sessions dirty too"
    );

    let list_result = tokio::time::timeout(
        Duration::from_secs(2),
        <JulieServerHandler as ServerHandler>::call_tool(
            &handler,
            CallToolRequestParams::new("manage_workspace").with_arguments(
                serde_json::json!({ "operation": "list" })
                    .as_object()
                    .expect("manage_workspace list args")
                    .clone(),
            ),
            RequestContext::new(NumberOrString::Number(19), service.peer().clone()),
        ),
    )
    .await
    .expect("env-startup primary request should not wait on roots/list");
    list_result?;

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
        "env-startup primary request should settle dirty state without rebinding"
    );

    drop(write_half);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_primary_request_cwd_filesystem_root_without_roots_support_errors_and_stays_unbound()
-> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

    let handler = JulieServerHandler::new_deferred_daemon_startup_hint_without_project_log(
        WorkspaceStartupHint {
            path: std::path::PathBuf::from("/"),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(Arc::clone(&embedding_service)),
        None,
    )
    .await?;

    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (_read_half, write_half) = tokio::io::split(client_transport);

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({
                "operation": "list"
            })
            .as_object()
            .expect("manage_workspace list args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(34), service.peer().clone()),
    )
    .await;

    let error = result.expect_err("filesystem root cwd fallback should be rejected");
    let message = error.to_string();
    assert!(
        message.contains("sensitive system path") && message.contains("JULIE_WORKSPACE"),
        "error should explain sensitive system path rejection: {message}"
    );
    assert!(
        handler.get_workspace().await?.is_none(),
        "filesystem root cwd fallback should not load a workspace"
    );
    assert!(
        !*handler.is_indexed.read().await,
        "filesystem root cwd fallback should not claim indexing"
    );
    assert_eq!(handler.current_workspace_id(), None);

    drop(write_half);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_initialized_cwd_without_roots_support_defers_auto_index() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn should_not_index_on_initialized() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let handler = JulieServerHandler::new_deferred_daemon_startup_hint(
        WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(Arc::clone(&embedding_service)),
        None,
    )
    .await?;

    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (_read_half, mut write_half) = tokio::io::split(client_transport);

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )
    .await?;

    sleep(Duration::from_millis(50)).await;

    assert!(
        handler.get_workspace().await?.is_none(),
        "cwd startup without roots support should not load the startup cwd on initialized"
    );
    assert!(
        !*handler.is_indexed.read().await,
        "cwd startup without roots support should defer auto-indexing until a primary tool request"
    );
    assert_eq!(handler.current_workspace_id(), None);

    let index_result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({
                "operation": "index"
            })
            .as_object()
            .expect("manage_workspace index args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(33), service.peer().clone()),
    )
    .await?;
    let index_text = extract_text(&index_result);
    assert!(
        index_text.contains("Workspace indexing complete")
            || index_text.contains("Workspace already indexed"),
        "first primary request should resume deferred startup indexing: {index_text}"
    );
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(startup_workspace_id.as_str()),
        "first primary request should bind the startup hint when roots are unsupported"
    );

    drop(write_half);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_first_primary_fast_search_with_deferred_cwd_indexes_before_search() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn deferred_fast_search_symbol() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

    let startup_path = startup_root.path().canonicalize()?;
    let startup_workspace_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let handler = JulieServerHandler::new_deferred_daemon_startup_hint(
        WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(Arc::clone(&embedding_service)),
        None,
    )
    .await?;

    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (_read_half, mut write_half) = tokio::io::split(client_transport);

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )
    .await?;

    sleep(Duration::from_millis(50)).await;
    assert!(handler.get_workspace().await?.is_none());
    assert_eq!(handler.current_workspace_id(), None);

    let search_result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("fast_search").with_arguments(
            serde_json::json!({
                "query": "deferred_fast_search_symbol",
                "limit": 5
            })
            .as_object()
            .expect("fast_search args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(35), service.peer().clone()),
    )
    .await?;

    let search_text = extract_text(&search_result);
    assert!(
        search_text.contains("deferred_fast_search_symbol"),
        "first primary search should synchronously complete deferred auto-indexing before searching: {search_text}"
    );
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(startup_workspace_id.as_str()),
        "first primary search should bind the startup hint when roots are unsupported"
    );

    drop(write_half);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
#[ignore = "daemon multi-workspace session/roots lifecycle (pool-backed); reworked in Phase 3d.3 registry rework"]
async fn test_target_fast_search_after_refresh_in_deferred_cwd_without_primary() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let target_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::write(
        startup_root.path().join("src/lib.rs"),
        "pub fn startup_marker_symbol() {}\n",
    )?;
    std::fs::create_dir_all(target_root.path().join("src"))?;
    std::fs::write(
        target_root.path().join("src/lib.rs"),
        "pub fn target_workspace_marker_symbol() {}\n",
    )?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

    let startup_path = startup_root.path().canonicalize()?;
    let handler = JulieServerHandler::new_deferred_daemon_startup_hint(
        WorkspaceStartupHint {
            path: startup_path,
            source: Some(WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(Arc::clone(&embedding_service)),
        None,
    )
    .await?;

    let (server_transport, client_transport) = tokio::io::duplex(512);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (_read_half, mut write_half) = tokio::io::split(client_transport);

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )
    .await?;

    sleep(Duration::from_millis(50)).await;
    assert!(handler.get_workspace().await?.is_none());
    assert_eq!(handler.current_workspace_id(), None);

    let target_path = target_root.path().canonicalize()?;
    let target_path_str = target_path.to_string_lossy().to_string();
    let target_workspace_id =
        crate::workspace::registry::generate_workspace_id(&target_path.to_string_lossy())?;
    daemon_db.upsert_workspace(&target_workspace_id, &target_path_str, "ready")?;

    let refresh_result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({
                "operation": "refresh",
                "workspace_id": target_workspace_id.clone(),
                "force": true
            })
            .as_object()
            .expect("manage_workspace refresh args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(36), service.peer().clone()),
    )
    .await?;
    let refresh_text = extract_text(&refresh_result);
    assert!(
        refresh_text.contains("Workspace Refresh:"),
        "target refresh should succeed before any primary tool call: {refresh_text}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "refreshing an explicit target must not bind the deferred cwd primary"
    );

    let search_result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("fast_search").with_arguments(
            serde_json::json!({
                "query": "target_workspace_marker_symbol",
                "limit": 5,
                "workspace": target_workspace_id.clone()
            })
            .as_object()
            .expect("target fast_search args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(37), service.peer().clone()),
    )
    .await?;

    let search_text = extract_text(&search_result);
    assert!(
        search_text.contains("target_workspace_marker_symbol"),
        "target search should use the target workspace storage path without requiring primary init: {search_text}"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "target search must keep the deferred cwd primary unbound"
    );

    drop(write_half);
    let _ = service.cancel().await;
    Ok(())
}
