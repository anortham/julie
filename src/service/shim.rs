use crate::service::client::{ServiceClient, connect_or_start, spawn_detached_service};
use anyhow::Context;
use julie_core::paths::RegistryPaths;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

pub async fn forward<R, W>(client: &ServiceClient, mut input: R, mut output: W) -> anyhow::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).await? == 0 { return Ok(()); }
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        let message: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("parse error: {e}") } });
                output.write_all(format!("{err}\n").as_bytes()).await?;
                continue;
            }
        };
        let method = message["method"].as_str().unwrap_or("").to_string();
        let is_request = !message["id"].is_null();
        let response = client.post_mcp(trimmed.as_bytes(), &method).await.context("POST /mcp")?;
        if !is_request { continue; }
        let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let body = response.text().await?;
        let reply = if content_type.starts_with("text/event-stream") {
            last_json_event(&body).unwrap_or_else(|| body.clone())
        } else {
            body
        };
        output.write_all(reply.trim().as_bytes()).await?;
        output.write_all(b"\n").await?;
        output.flush().await?;
    }
}

fn last_json_event(sse: &str) -> Option<String> {
    sse.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .filter(|d| serde_json::from_str::<serde_json::Value>(d).map(|v| v.get("id").is_some()).unwrap_or(false))
        .last()
        .map(str::to_owned)
}

pub async fn run_stdio_shim() -> anyhow::Result<()> {
    let paths = RegistryPaths::try_new().context("resolve Julie home")?;
    let client = connect_or_start(&paths, spawn_detached_service).await?;
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    forward(&client, stdin, stdout).await
}
