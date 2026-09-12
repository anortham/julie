use crate::service::client::{ServiceClient, connect_or_start, spawn_detached_service};
use anyhow::Context;
use julie_core::paths::RegistryPaths;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

pub struct ClientStamp {
    session: String,
    client: Option<String>,
}

impl ClientStamp {
    pub fn new(session: impl Into<String>) -> Self {
        Self {
            session: session.into(),
            client: None,
        }
    }

    pub fn observe(&mut self, message: &mut serde_json::Value) {
        match message["method"].as_str() {
            Some("initialize") => {
                let info = &message["params"]["clientInfo"];
                if let Some(name) = info["name"].as_str() {
                    let version = info["version"].as_str().unwrap_or("");
                    self.client = Some(format!("{name}/{version}"));
                }
            }
            Some("tools/call") => {
                message["params"]["_meta"]["julie"] = serde_json::json!({
                    "client": self.client.clone().unwrap_or_default(),
                    "session": self.session,
                });
            }
            _ => {}
        }
    }
}

pub async fn forward<R, W>(
    paths: &RegistryPaths,
    spawn: &(impl Fn() -> std::io::Result<()>),
    mut client: ServiceClient,
    mut input: R,
    mut output: W,
) -> anyhow::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut stamp = ClientStamp::new(uuid::Uuid::new_v4().to_string());
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut message: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": format!("parse error: {e}") } });
                output.write_all(format!("{err}\n").as_bytes()).await?;
                continue;
            }
        };
        stamp.observe(&mut message);
        let method = message["method"].as_str().unwrap_or("").to_string();
        let is_request = !message["id"].is_null();
        let body = serde_json::to_vec(&message)?;
        let response = match client.post_mcp(&body, &method).await {
            Ok(r) => r,
            Err(_) => {
                let retried = match connect_or_start(paths, spawn).await {
                    Ok(fresh) => {
                        client = fresh;
                        client
                            .post_mcp(&body, &method)
                            .await
                            .map_err(|e| e.to_string())
                    }
                    Err(e) => Err(e.to_string()),
                };
                match retried {
                    Ok(r) => r,
                    Err(reason) => {
                        if is_request {
                            let err = serde_json::json!({ "jsonrpc": "2.0", "id": message["id"], "error": { "code": -32000, "message": format!("julie service unavailable: {reason}") } });
                            output.write_all(format!("{err}\n").as_bytes()).await?;
                            output.flush().await?;
                        }
                        continue;
                    }
                }
            }
        };
        if !is_request {
            continue;
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
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
        .filter(|d| {
            serde_json::from_str::<serde_json::Value>(d)
                .map(|v| v.get("id").is_some())
                .unwrap_or(false)
        })
        .last()
        .map(str::to_owned)
}

pub async fn run_stdio_shim() -> anyhow::Result<()> {
    let paths = RegistryPaths::try_new().context("resolve Julie home")?;
    let client = connect_or_start(&paths, spawn_detached_service).await?;
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    forward(&paths, &spawn_detached_service, client, stdin, stdout).await
}
