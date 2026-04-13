use anyhow::Result;
use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, NumberOrString, ServerJsonRpcMessage, ServerRequest},
    service::{RequestContext, serve_directly},
};
use std::fs;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::metrics::QueryMetricsTool;

fn extract_text_from_result(result: &CallToolResult) -> String {
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

#[tokio::test(flavor = "multi_thread")]
async fn test_query_metrics_primary_uses_rebound_current_primary_store() -> Result<()> {
    let first_root = TempDir::new()?;
    let rebound_root = TempDir::new()?;

    fs::create_dir_all(first_root.path().join(".git"))?;
    fs::create_dir_all(rebound_root.path().join(".git"))?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(
            Some(first_root.path().to_string_lossy().to_string()),
            true,
        )
        .await?;

    let rebound_path = rebound_root.path().canonicalize()?;
    let rebound_id =
        crate::workspace::registry::generate_workspace_id(&rebound_path.to_string_lossy())?;
    handler.set_current_primary_binding(rebound_id.clone(), rebound_path.clone());

    let rebound_db_path = handler.workspace_db_file_path_for(&rebound_id).await?;
    fs::create_dir_all(rebound_db_path.parent().expect("rebound db parent"))?;
    let rebound_db = crate::database::SymbolDatabase::new(&rebound_db_path)?;
    rebound_db.insert_tool_call(
        "session-1",
        "fast_search",
        4.0,
        Some(2),
        Some(1234),
        Some(88),
        true,
        None,
    )?;

    let first_id = crate::workspace::registry::generate_workspace_id(
        &first_root.path().canonicalize()?.to_string_lossy(),
    )?;
    let first_db_path = handler
        .workspace_root
        .join(".julie")
        .join("indexes")
        .join(&first_id)
        .join("db")
        .join("symbols.db");
    fs::create_dir_all(first_db_path.parent().expect("first db parent"))?;
    let first_db = crate::database::SymbolDatabase::new(&first_db_path)?;
    first_db.insert_tool_call(
        "session-1",
        "deep_dive",
        9.0,
        Some(1),
        Some(9999),
        Some(77),
        true,
        None,
    )?;

    let tool = QueryMetricsTool {
        category: "history".to_string(),
        order: "desc".to_string(),
        limit: 20,
        workspace: Some("primary".to_string()),
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text_from_result(&result);
    assert!(
        text.contains("fast_search"),
        "expected rebound primary history, got: {text}"
    );
    assert!(
        !text.contains("deep_dive"),
        "should not read stale loaded primary history: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_neutral_gap_primary_query_metrics_fails_without_primary_identity() -> Result<()> {
    let first_root = TempDir::new()?;

    fs::create_dir_all(first_root.path().join(".git"))?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(
            Some(first_root.path().to_string_lossy().to_string()),
            true,
        )
        .await?;

    let first_id = crate::workspace::registry::generate_workspace_id(
        &first_root.path().canonicalize()?.to_string_lossy(),
    )?;
    let first_db_path = handler.workspace_db_file_path_for(&first_id).await?;
    fs::create_dir_all(first_db_path.parent().expect("first db parent"))?;
    let first_db = crate::database::SymbolDatabase::new(&first_db_path)?;
    first_db.insert_tool_call(
        "session-1",
        "deep_dive",
        9.0,
        Some(1),
        Some(9999),
        Some(77),
        true,
        None,
    )?;

    handler.publish_loaded_workspace_swap_intent_for_test();

    let tool = QueryMetricsTool {
        category: "history".to_string(),
        order: "desc".to_string(),
        limit: 20,
        workspace: Some("primary".to_string()),
    };

    let err = tool
        .call_tool(&handler)
        .await
        .expect_err("neutral gap should reject primary metrics queries");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_metrics_primary_cold_start_returns_index_first_error() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let tool = QueryMetricsTool {
        category: "history".to_string(),
        order: "desc".to_string(),
        limit: 20,
        workspace: Some("primary".to_string()),
    };

    let err = tool
        .call_tool(&handler)
        .await
        .expect_err("cold start should report that indexing has not happened yet");

    assert!(
        err.to_string()
            .contains("Run manage_workspace(operation=\"index\") first"),
        "unexpected error: {err:#}"
    );
    assert!(
        !err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "cold start should not be classified as a swap gap: {err:#}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_metrics_primary_after_roots_bind_attaches_daemon_workspace() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let startup_root = temp_dir.path().join("startup");
    let rebound_root = temp_dir.path().join("rebound");
    fs::create_dir_all(startup_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(startup_root.join("src/lib.rs"), "pub fn startup() {}\n")?;
    fs::write(rebound_root.join("src/lib.rs"), "pub fn rebound() {}\n")?;

    let daemon_db = std::sync::Arc::new(crate::daemon::database::DaemonDatabase::open(
        &temp_dir.path().join("daemon.db"),
    )?);
    let pool = std::sync::Arc::new(crate::daemon::workspace_pool::WorkspacePool::new(
        indexes_dir,
        Some(std::sync::Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let startup_path = startup_root.canonicalize()?;
    let startup_id =
        crate::workspace::registry::generate_workspace_id(&startup_path.to_string_lossy())?;
    let startup_ws = pool.get_or_init(&startup_id, startup_path.clone()).await?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_id =
        crate::workspace::registry::generate_workspace_id(&rebound_path.to_string_lossy())?;
    let _rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;

    let rebound_db_path = startup_ws
        .index_root_override
        .as_ref()
        .expect("daemon workspace should use shared index override")
        .parent()
        .expect("workspace index root")
        .join(&rebound_id)
        .join("db")
        .join("symbols.db");
    fs::create_dir_all(rebound_db_path.parent().expect("rebound db parent"))?;
    let rebound_db = crate::database::SymbolDatabase::new(&rebound_db_path)?;
    rebound_db.insert_tool_call(
        "session-1",
        "fast_search",
        4.0,
        Some(2),
        Some(1234),
        Some(88),
        true,
        None,
    )?;

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(std::sync::Arc::clone(&daemon_db)),
        Some(startup_id.clone()),
        None,
        None,
        None,
        None,
        Some(std::sync::Arc::clone(&pool)),
    )
    .await?;
    handler.set_client_supports_workspace_roots_for_test(true);

    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let roots_reply = async {
        match read_server_message(&mut lines).await? {
            ServerJsonRpcMessage::Request(request) => match request.request {
                ServerRequest::ListRootsRequest(_) => {
                    send_json_line(
                        &mut write_half,
                        &serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request.id,
                            "result": {
                                "roots": [{ "uri": format!("file://{}", rebound_path.to_string_lossy()) }]
                            }
                        }),
                    )
                    .await
                }
                other => panic!("unexpected server request: {other:?}"),
            },
            other => panic!("unexpected server message: {other:?}"),
        }
    };

    let query = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("query_metrics").with_arguments(
            serde_json::json!({
                "category": "history",
                "workspace": "primary"
            })
            .as_object()
            .expect("query_metrics args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(7), service.peer().clone()),
    );
    let (roots_result, query_result) = tokio::join!(roots_reply, query);
    roots_result?;
    let result = query_result?;
    let text = extract_text_from_result(&result);

    assert!(
        text.contains("fast_search"),
        "expected rebound workspace history, got: {text}"
    );
    assert!(
        handler.was_workspace_attached_in_session(&rebound_id).await,
        "roots-bound current primary should be attached through the daemon workspace pool"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_metrics_session_does_not_trigger_roots_resolution() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let startup_root = temp_dir.path().join("startup");
    fs::create_dir_all(startup_root.join("src"))?;
    fs::write(startup_root.join("src/lib.rs"), "pub fn startup() {}\n")?;

    let handler = JulieServerHandler::new(startup_root.canonicalize()?).await?;
    handler.set_client_supports_workspace_roots_for_test(true);

    let (server_transport, client_transport) = tokio::io::duplex(64);
    drop(client_transport);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);

    let result = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("query_metrics").with_arguments(
            serde_json::json!({ "category": "session" })
                .as_object()
                .expect("query_metrics args")
                .clone(),
        ),
        RequestContext::new(NumberOrString::Number(8), service.peer().clone()),
    )
    .await?;
    let text = extract_text_from_result(&result);

    assert!(
        !text.is_empty(),
        "session metrics should succeed without roots/list"
    );
    assert_eq!(
        handler.current_workspace_id(),
        None,
        "session-only query_metrics should not trigger primary roots resolution"
    );

    let _ = service.cancel().await;
    Ok(())
}
