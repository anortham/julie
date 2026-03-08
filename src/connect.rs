//! Connect command: auto-start daemon + stdio↔HTTP bridge.
//!
//! `julie-server connect` ensures the daemon is running (starting it as a
//! background child process if needed), registers the current workspace,
//! and bridges stdin/stdout to the daemon's MCP HTTP endpoint.
//!
//! This lets MCP clients use the `command` transport type in `.mcp.json`
//! while Julie runs as a persistent daemon that survives session exits.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tracing::{debug, error, info, warn};

use crate::daemon::{is_daemon_running, julie_home, pid_file_path};

// Backoff schedule for polling daemon health (total ~5s)
pub(crate) const BACKOFF_MS: &[u64] = &[50, 100, 200, 400, 800, 1600, 2000];

/// Main entry point for the `connect` command.
///
/// 1. Ensures the daemon is running (starts it if not)
/// 2. Registers the workspace with the daemon
/// 3. Bridges stdio ↔ HTTP for MCP communication
/// 4. Falls back to direct stdio MCP mode if daemon can't start
pub async fn run_connect(port: u16, workspace_root: PathBuf) -> Result<()> {
    info!("Connect command: port={}, workspace={:?}", port, workspace_root);

    // Step 1: Ensure daemon is running
    let daemon_port = match ensure_daemon_running(port).await {
        Ok(p) => {
            info!("Daemon available on port {}", p);
            p
        }
        Err(e) => {
            warn!(
                "Failed to start daemon, falling back to stdio mode: {}",
                e
            );
            eprintln!(
                "Warning: Could not start daemon ({}). Falling back to direct stdio mode.",
                e
            );
            return crate::stdio::run_stdio_mode(workspace_root).await;
        }
    };

    // Step 2: Register workspace with the daemon
    let workspace_id = match register_workspace(daemon_port, &workspace_root).await {
        Ok(id) => {
            info!("Workspace registered: {}", id);
            id
        }
        Err(e) => {
            warn!(
                "Failed to register workspace, falling back to stdio mode: {}",
                e
            );
            eprintln!(
                "Warning: Could not register workspace ({}). Falling back to direct stdio mode.",
                e
            );
            return crate::stdio::run_stdio_mode(workspace_root).await;
        }
    };

    // Step 3: Bridge stdio ↔ HTTP
    let mcp_url = format!("http://localhost:{}/mcp/{}", daemon_port, workspace_id);
    info!("Bridging stdio to MCP endpoint: {}", mcp_url);

    run_stdio_bridge(&mcp_url).await
}

/// Ensure the daemon is running, starting it if necessary.
///
/// Returns the port the daemon is listening on.
pub(crate) async fn ensure_daemon_running(requested_port: u16) -> Result<u16> {
    let pid_path = pid_file_path()?;

    // Check if daemon is already running
    if let Some(info) = is_daemon_running(&pid_path) {
        info!(
            "Daemon already running (PID {}, port {})",
            info.pid, info.port
        );
        return Ok(info.port);
    }

    // Not running — spawn it
    info!("Daemon not running, spawning on port {}", requested_port);
    spawn_daemon(requested_port)?;

    // Poll health endpoint until ready
    wait_for_daemon_health(requested_port).await?;

    Ok(requested_port)
}

/// Spawn the daemon as a detached background child process.
///
/// Uses `std::process::Command` to run `julie-server daemon start --foreground --port <port>`.
/// stdin is null, stdout/stderr go to log files (not inherited — those belong to the MCP bridge).
fn spawn_daemon(port: u16) -> Result<()> {
    let exe = std::env::current_exe().context("Failed to determine current executable path")?;

    // Set up log files for the daemon's stdout/stderr
    let home = julie_home()?;
    let logs_dir = home.join("logs");
    std::fs::create_dir_all(&logs_dir)
        .with_context(|| format!("Failed to create logs directory {:?}", logs_dir))?;

    let stdout_log = std::fs::File::create(logs_dir.join("daemon-stdout.log"))
        .context("Failed to create daemon stdout log")?;
    let stderr_log = std::fs::File::create(logs_dir.join("daemon-stderr.log"))
        .context("Failed to create daemon stderr log")?;

    let child = std::process::Command::new(exe)
        .args(["daemon", "start", "--port", &port.to_string(), "--foreground"])
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log))
        .spawn()
        .context("Failed to spawn daemon process")?;

    info!("Daemon child spawned (PID {})", child.id());
    // We intentionally drop the Child handle here — the daemon runs independently.
    // The PID file is the canonical way to track it.

    Ok(())
}

/// Poll the daemon's health endpoint with exponential backoff.
///
/// Returns Ok(()) when the daemon is ready, or Err if it doesn't respond
/// within the backoff schedule (~5s total).
async fn wait_for_daemon_health(port: u16) -> Result<()> {
    let url = format!("http://localhost:{}/api/health", port);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .context("Failed to create HTTP client")?;

    for (i, &delay_ms) in BACKOFF_MS.iter().enumerate() {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;

        debug!(
            "Health check attempt {}/{}: GET {}",
            i + 1,
            BACKOFF_MS.len(),
            url
        );

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!("Daemon health check passed after {} attempts", i + 1);
                return Ok(());
            }
            Ok(resp) => {
                debug!("Health check returned status {}", resp.status());
            }
            Err(e) => {
                debug!("Health check failed: {}", e);
            }
        }
    }

    bail!(
        "Daemon did not become healthy within {}ms",
        BACKOFF_MS.iter().sum::<u64>()
    )
}

/// Register a workspace with the daemon via POST /api/projects.
///
/// Returns the workspace_id from the daemon's response.
/// This is idempotent — if the workspace is already registered, the daemon
/// returns 409 Conflict with the existing project info.
pub(crate) async fn register_workspace(port: u16, workspace_root: &PathBuf) -> Result<String> {
    let url = format!("http://localhost:{}/api/projects", port);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to create HTTP client")?;

    let body = serde_json::json!({
        "path": workspace_root.to_string_lossy()
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("Failed to POST {}", url))?;

    let status = resp.status();

    // 201 Created or 409 Conflict (already registered) — both are fine
    if status == reqwest::StatusCode::CREATED || status == reqwest::StatusCode::CONFLICT {
        let json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse project registration response")?;

        let workspace_id = json["workspace_id"]
            .as_str()
            .context("Response missing workspace_id field")?
            .to_string();

        debug!(
            "Workspace registration: status={}, workspace_id={}",
            status, workspace_id
        );
        return Ok(workspace_id);
    }

    let error_body = resp.text().await.unwrap_or_default();
    bail!(
        "Workspace registration failed (HTTP {}): {}",
        status,
        error_body
    )
}

/// Bridge stdin/stdout to the daemon's MCP HTTP endpoint.
///
/// Reads newline-delimited JSON-RPC messages from stdin, POSTs each to
/// the MCP endpoint, and writes the response back to stdout.
///
/// The MCP Streamable HTTP transport works as:
/// - POST with JSON-RPC body → response is either direct JSON or SSE stream
/// - For SSE, we forward events as they arrive
///
/// Exits on stdin EOF or unrecoverable bridge error.
async fn run_stdio_bridge(mcp_url: &str) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300)) // MCP requests can be long-running
        .build()
        .context("Failed to create HTTP client for bridge")?;

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    // Track the MCP session ID — the server returns it in Mcp-Session-Id header
    let mut session_id: Option<String> = None;

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .context("Failed to read from stdin")?;

        if bytes_read == 0 {
            // EOF — client disconnected
            info!("stdin EOF, bridge shutting down");

            // Send DELETE to tear down the MCP session if we have a session ID
            if let Some(ref sid) = session_id {
                debug!("Sending session teardown DELETE for session {}", sid);
                let _ = client
                    .delete(mcp_url)
                    .header("Mcp-Session-Id", sid)
                    .send()
                    .await;
            }
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        debug!("Bridge → daemon: {}", trimmed);

        // Build the POST request
        let mut req = client
            .post(mcp_url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream");

        // Include session ID if we have one
        if let Some(ref sid) = session_id {
            req = req.header("Mcp-Session-Id", sid);
        }

        let resp = match req.body(trimmed.to_string()).send().await {
            Ok(r) => r,
            Err(e) => {
                error!("Bridge HTTP error: {}", e);
                // Write a JSON-RPC error response so the client knows
                let error_resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "error": {
                        "code": -32000,
                        "message": format!("Bridge error: {}", e)
                    },
                    "id": null
                });
                let mut out = serde_json::to_vec(&error_resp)?;
                out.push(b'\n');
                stdout.write_all(&out).await?;
                stdout.flush().await?;
                continue;
            }
        };

        // Capture session ID from response headers
        if session_id.is_none() {
            if let Some(sid) = resp.headers().get("mcp-session-id") {
                if let Ok(sid_str) = sid.to_str() {
                    session_id = Some(sid_str.to_string());
                    debug!("Captured MCP session ID: {}", sid_str);
                }
            }
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        if content_type.contains("text/event-stream") {
            // SSE response — forward events as JSON-RPC messages
            debug!("SSE response stream");
            let mut stream = resp.bytes_stream();
            use futures::StreamExt;

            let mut sse_buffer = String::new();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        sse_buffer.push_str(&text);

                        // Process complete SSE events in the buffer
                        while let Some(event_end) = sse_buffer.find("\n\n") {
                            let event = sse_buffer[..event_end].to_string();
                            sse_buffer = sse_buffer[event_end + 2..].to_string();

                            // Extract data lines from SSE event
                            for sse_line in event.lines() {
                                if let Some(data) = sse_line.strip_prefix("data: ") {
                                    debug!("Bridge ← daemon (SSE): {}", data);
                                    let mut out = data.as_bytes().to_vec();
                                    out.push(b'\n');
                                    stdout.write_all(&out).await?;
                                    stdout.flush().await?;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("SSE stream error: {}", e);
                        break;
                    }
                }
            }
        } else {
            // Direct JSON response — forward to stdout
            let body = resp.bytes().await.context("Failed to read response body")?;
            if !body.is_empty() {
                debug!(
                    "Bridge ← daemon: {}",
                    String::from_utf8_lossy(&body)
                );
                stdout.write_all(&body).await?;
                // Ensure newline-delimited
                if !body.ends_with(b"\n") {
                    stdout.write_all(b"\n").await?;
                }
                stdout.flush().await?;
            }
        }
    }

    info!("Bridge exited cleanly");
    Ok(())
}
