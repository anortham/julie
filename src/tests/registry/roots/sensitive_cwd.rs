use super::*;

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
