use super::*;

#[tokio::test]
#[ignore = "daemon multi-workspace session/roots lifecycle (pool-backed); reworked in Phase 3d.3 registry rework"]
async fn test_roots_list_changed_resolves_deferred_auto_index() -> Result<()> {
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
    let (server_transport, client_transport) = tokio::io::duplex(512);
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

    tokio::task::yield_now().await;

    assert_eq!(
        handler.current_workspace_id(),
        None,
        "cwd startup should remain unbound after on_initialized"
    );
    assert!(
        !*handler.is_indexed.read().await,
        "auto-indexing should be deferred after cwd startup"
    );

    send_json_line(
        &mut write_half,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/roots/list_changed"
        }),
    )
    .await?;

    let roots_paths = [roots_root.path()];
    tokio::time::timeout(
        Duration::from_secs(10),
        answer_roots_request(&mut lines, &mut write_half, &roots_paths),
    )
    .await
    .expect("roots_list_changed with deferred auto-index should eagerly send roots/list")?;

    let roots_workspace_id = crate::workspace::registry::generate_workspace_id(
        &roots_root.path().canonicalize()?.to_string_lossy(),
    )?;

    let indexed = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if handler.current_workspace_id().as_deref() == Some(roots_workspace_id.as_str()) {
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
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("roots_list_changed should eagerly resolve deferred auto-indexing")?;
    assert!(
        indexed,
        "roots_list_changed with pending deferred auto-index should bind workspace and index"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

async fn build_cwd_path_handler(
    path: std::path::PathBuf,
) -> Result<(
    JulieServerHandler,
    rmcp::service::RunningService<rmcp::RoleServer, JulieServerHandler>,
    tokio::io::WriteHalf<tokio::io::DuplexStream>,
    tempfile::TempDir,
)> {
    let indexes_dir = tempfile::tempdir()?;
    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);

    let handler = JulieServerHandler::new_deferred_daemon_startup_hint_without_project_log(
        WorkspaceStartupHint {
            path,
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
    Ok((handler, service, write_half, indexes_dir))
}

async fn expect_cwd_path_rejected_as_sensitive(path: std::path::PathBuf) -> Result<()> {
    let display = path.display().to_string();
    let (handler, service, write_half, _indexes_dir) = build_cwd_path_handler(path.clone()).await?;

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({"operation": "list"})
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(34), service.peer().clone()),
    )
    .await;

    let error = result.expect_err(&format!("sensitive cwd path {display} should be rejected"));
    let message = error.to_string();
    assert!(
        message.contains("sensitive system path") && message.contains("JULIE_WORKSPACE"),
        "error should explain sensitive system path rejection for {display}: {message}"
    );
    assert!(
        handler.get_workspace().await?.is_none(),
        "sensitive cwd {display} should not load a workspace"
    );
    assert!(
        !*handler.is_indexed.read().await,
        "sensitive cwd {display} should not claim indexing"
    );
    assert_eq!(handler.current_workspace_id(), None);

    drop(write_half);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test]
async fn test_primary_request_cwd_home_dir_is_rejected_as_sensitive() -> Result<()> {
    let Some(home) = dirs::home_dir() else {
        eprintln!("skipping: dirs::home_dir() returned None");
        return Ok(());
    };
    expect_cwd_path_rejected_as_sensitive(home).await
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn test_primary_request_cwd_users_root_is_rejected_as_sensitive() -> Result<()> {
    expect_cwd_path_rejected_as_sensitive(std::path::PathBuf::from("/Users")).await
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_primary_request_cwd_home_root_is_rejected_as_sensitive() -> Result<()> {
    expect_cwd_path_rejected_as_sensitive(std::path::PathBuf::from("/home")).await
}

#[cfg(windows)]
#[tokio::test]
async fn test_primary_request_cwd_windows_users_is_rejected_as_sensitive() -> Result<()> {
    expect_cwd_path_rejected_as_sensitive(std::path::PathBuf::from(r"C:\Users")).await
}

#[cfg(windows)]
#[tokio::test]
async fn test_primary_request_cwd_windows_root_is_rejected_as_sensitive() -> Result<()> {
    expect_cwd_path_rejected_as_sensitive(std::path::PathBuf::from(r"C:\Windows")).await
}

#[cfg(windows)]
#[tokio::test]
async fn test_primary_request_cwd_windows_system32_is_rejected_as_sensitive() -> Result<()> {
    expect_cwd_path_rejected_as_sensitive(std::path::PathBuf::from(r"C:\Windows\System32")).await
}

#[cfg(windows)]
#[tokio::test]
async fn test_primary_request_cwd_windows_system32_lowercase_is_rejected_as_sensitive() -> Result<()>
{
    expect_cwd_path_rejected_as_sensitive(std::path::PathBuf::from(r"c:\windows\system32")).await
}

#[tokio::test]
async fn test_primary_request_cwd_normal_project_dir_is_not_rejected_by_sensitive_guard()
-> Result<()> {
    let project_root = tempfile::tempdir()?;
    std::fs::write(
        project_root.path().join("lib.rs"),
        "pub fn marker_for_sensitive_guard_positive_case() {}\n",
    )?;
    let path = project_root.path().to_path_buf();
    let (handler, service, write_half, _indexes_dir) = build_cwd_path_handler(path.clone()).await?;

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({"operation": "list"})
                .as_object()
                .expect("manage_workspace list args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(91), service.peer().clone()),
    )
    .await;

    if let Err(err) = &result {
        let message = err.to_string();
        assert!(
            !message.contains("sensitive system path"),
            "non-sensitive cwd {} should not trip the sensitive guard, got: {message}",
            path.display()
        );
    }

    drop(write_half);
    let _ = service.cancel().await;
    Ok(())
}
