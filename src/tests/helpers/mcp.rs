/// Extracts text from MCP `CallToolResult` content blocks.
pub fn call_tool_result_text(result: &crate::mcp_compat::CallToolResult) -> String {
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

pub async fn send_json_line(
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    value: &serde_json::Value,
) {
    use tokio::io::AsyncWriteExt;

    writer
        .write_all(serde_json::to_string(value).unwrap().as_bytes())
        .await
        .unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
}

pub async fn read_server_message<R>(
    lines: &mut tokio::io::Lines<R>,
) -> rmcp::model::ServerJsonRpcMessage
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let line = lines
        .next_line()
        .await
        .unwrap()
        .expect("server should emit a JSON-RPC message line");
    serde_json::from_str(&line).unwrap()
}

pub async fn answer_next_list_roots_request<R>(
    lines: &mut tokio::io::Lines<R>,
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    roots: &[&std::path::Path],
) where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let message = read_server_message(lines).await;
    match message {
        rmcp::model::ServerJsonRpcMessage::Request(request) => match request.request {
            rmcp::model::ServerRequest::ListRootsRequest(_) => {
                let roots = roots
                    .iter()
                    .map(|path| {
                        serde_json::json!({
                            "uri": format!("file://{}", path.to_string_lossy())
                        })
                    })
                    .collect::<Vec<_>>();
                send_json_line(
                    writer,
                    &serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": request.id,
                        "result": { "roots": roots }
                    }),
                )
                .await;
            }
            other => panic!("unexpected server request: {other:?}"),
        },
        other => panic!("unexpected server message: {other:?}"),
    }
}
