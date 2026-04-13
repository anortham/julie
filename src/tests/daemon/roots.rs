use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use anyhow::Result;
use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, NumberOrString, ServerJsonRpcMessage, ServerRequest},
    service::{RequestContext, serve_directly},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::sleep;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::JulieServerHandler;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

async fn send_json_line(
    writer: &mut (impl AsyncWriteExt + Unpin),
    value: &serde_json::Value,
) -> Result<()> {
    writer
        .write_all(serde_json::to_string(value)?.as_bytes())
        .await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}

async fn read_server_message(
    lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
) -> Result<ServerJsonRpcMessage> {
    let line = lines
        .next_line()
        .await?
        .expect("server should emit a JSON-RPC message line");
    Ok(serde_json::from_str(&line)?)
}

async fn answer_roots_request(
    lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
    writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
    roots: &[&Path],
) -> Result<()> {
    let message = tokio::time::timeout(Duration::from_secs(10), read_server_message(lines))
        .await
        .expect("server should request roots during the first primary-scoped call")?;

    match message {
        ServerJsonRpcMessage::Request(request) => match request.request {
            ServerRequest::ListRootsRequest(_) => {
                send_json_line(
                    writer,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request.id,
                        "result": {
                            "roots": roots.iter().map(|root| serde_json::json!({
                                "uri": format!("file://{}", root.to_string_lossy())
                            })).collect::<Vec<_>>()
                        }
                    }),
                )
                .await?;
                Ok(())
            }
            other => {
                panic!("unexpected server request during first primary-scoped call: {other:?}")
            }
        },
        other => panic!("unexpected server message during first primary-scoped call: {other:?}"),
    }
}

async fn answer_roots_request_error(
    lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
    writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
    error_message: &str,
) -> Result<()> {
    let request_message = tokio::time::timeout(Duration::from_secs(10), read_server_message(lines))
        .await
        .expect("server should request roots during the first primary-scoped call")?;

    match request_message {
        ServerJsonRpcMessage::Request(request) => match request.request {
            ServerRequest::ListRootsRequest(_) => {
                send_json_line(
                    writer,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request.id,
                        "error": {
                            "code": -32000,
                            "message": error_message,
                        }
                    }),
                )
                .await?;
                Ok(())
            }
            other => {
                panic!("unexpected server request during first primary-scoped call: {other:?}")
            }
        },
        other => panic!("unexpected server message during first primary-scoped call: {other:?}"),
    }
}

#[tokio::test]
async fn test_initialized_weak_cwd_does_not_probe_roots_before_first_request() -> Result<()> {
    let indexes_dir = tempfile::tempdir()?;
    let startup_root = tempfile::tempdir()?;
    let roots_root = tempfile::tempdir()?;
    std::fs::create_dir_all(startup_root.path().join("src"))?;
    std::fs::create_dir_all(roots_root.path().join("src"))?;
    std::fs::write(startup_root.path().join("src/lib.rs"), "pub fn from_startup() {}\n")?;
    std::fs::write(roots_root.path().join("src/lib.rs"), "pub fn from_roots() {}\n")?;

    let daemon_db_path = indexes_dir.path().join("daemon.db");
    let daemon_db = Arc::new(DaemonDatabase::open(&daemon_db_path)?);
    let embedding_service = Arc::new(EmbeddingService::initializing());
    embedding_service.publish_unavailable("test: embeddings disabled".to_string(), None);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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

    match tokio::time::timeout(Duration::from_millis(250), read_server_message(&mut lines)).await {
        Ok(Ok(message)) => {
            panic!(
                "weak cwd startup should defer roots/list until the first primary-scoped request, got: {message:?}"
            );
        }
        Ok(Err(err)) => return Err(err),
        Err(_) => {}
    }

    assert_eq!(
        handler.current_workspace_id(),
        None,
        "weak cwd startup should remain unbound after on_initialized"
    );
    assert!(
        !*handler.is_indexed.read().await,
        "weak cwd startup should keep auto-indexing deferred until request-time resolution"
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
        RequestContext::new(NumberOrString::Number(300), service.peer().clone()),
    );
    let (roots_result, list_result) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(roots_reply, list_future)
    })
    .await
    .expect("request-time roots resolution should finish");
    roots_result?;
    let result = list_result?;

    let roots_workspace_id = crate::workspace::registry::generate_workspace_id(
        &roots_root.path().canonicalize()?.to_string_lossy(),
    )?;
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(roots_workspace_id.as_str()),
        "the first primary-scoped request should bind the real roots workspace"
    );
    assert_eq!(handler.current_workspace_root(), roots_root.path().canonicalize()?);
    assert!(
        extract_text(&result).contains(&roots_workspace_id),
        "manage_workspace list should report the roots-bound primary"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

fn extract_text(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn wait_for_session_count(daemon_db: &DaemonDatabase, workspace_id: &str, expected: i64) {
    let mut last = None;
    for _ in 0..100 {
        if let Ok(Some(row)) = daemon_db.get_workspace(workspace_id) {
            if row.session_count == expected {
                return;
            }
            last = Some(row.session_count);
        }
        sleep(Duration::from_millis(50)).await;
    }

    let last = last.unwrap_or(-1);
    panic!(
        "Timed out waiting for workspace '{workspace_id}' session_count={expected}, last observed={last}"
    );
}

#[tokio::test]
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
    pool.disconnect_session(&startup_workspace_id).await;
    wait_for_session_count(&daemon_db, &startup_workspace_id, 0).await;
    Ok(())
}

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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
        None,
        Some(Arc::clone(&embedding_service)),
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
    assert!(
        extract_text(&secondary_result).contains("secondary_root"),
        "secondary-scoped request should keep working while roots are dirty"
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
        None,
        Some(Arc::clone(&embedding_service)),
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
    handler.mark_workspace_active(&secondary_id).await;

    assert_eq!(
        handler.current_workspace_id(),
        None,
        "reference-scoped first request should start before any primary bind"
    );
    assert!(handler.is_workspace_active(&secondary_id).await);

    let search_result = tokio::time::timeout(Duration::from_secs(10), async {
        <JulieServerHandler as ServerHandler>::call_tool(
            &handler,
            CallToolRequestParams::new("fast_search").with_arguments(
                serde_json::json!({
                    "query": "secondary_first_request_symbol",
                    "workspace": secondary_id,
                    "search_target": "definitions",
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
        None,
        Some(Arc::clone(&embedding_service)),
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
                    "search_target": "definitions",
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
        None,
        Some(Arc::clone(&embedding_service)),
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
        None,
        Some(Arc::clone(&embedding_service)),
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.path().to_path_buf(),
        Some(Arc::clone(&daemon_db)),
        None,
        Some(Arc::clone(&embedding_service)),
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
            source: Some(WorkspaceStartupSource::Env),
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
