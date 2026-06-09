use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, NumberOrString, ServerJsonRpcMessage, ServerRequest},
    service::{RequestContext, serve_directly},
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::sleep;

use crate::handler::JulieServerHandler;
use crate::registry::database::DaemonDatabase;
use crate::registry::embedding_service::EmbeddingService;
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

mod deferred_auto_index_sensitive;
mod initial_binding;
mod list_changed_refresh;
mod secondary_targets;
mod startup_deferral;

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
