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

use crate::daemon::{daemon_stop, is_binary_newer_than_daemon, is_daemon_running, julie_home, pid_file_path};

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

    // Step 3: Bridge stdio ↔ HTTP (with reconnect on daemon crash)
    let mcp_url = format!("http://localhost:{}/mcp/{}", daemon_port, workspace_id);
    info!("Bridging stdio to MCP endpoint: {}", mcp_url);

    run_stdio_bridge(&mcp_url, port, &workspace_root).await
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

        // Check if the binary has been rebuilt since the daemon started
        if is_binary_newer_than_daemon(info.port).await {
            info!("Binary is newer than running daemon — restarting");
            eprintln!("Julie: binary is newer than running daemon, restarting...");
            daemon_stop()?;
        } else {
            return Ok(info.port);
        }
    }

    // Not running (or just stopped for restart) — spawn it
    info!("Spawning daemon on port {}", requested_port);
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

    let stdout_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs_dir.join("daemon-stdout.log"))
        .context("Failed to open daemon stdout log")?;
    let stderr_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs_dir.join("daemon-stderr.log"))
        .context("Failed to open daemon stderr log")?;

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
pub(crate) async fn register_workspace(port: u16, workspace_root: &std::path::Path) -> Result<String> {
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

/// Maximum reconnect attempts before giving up and returning errors to client.
const MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// Maximum SSE buffer size (1MB) to prevent unbounded memory growth.
const MAX_SSE_BUFFER_SIZE: usize = 1_048_576;

/// Bridge stdin/stdout to the daemon's MCP HTTP endpoint.
///
/// Reads newline-delimited JSON-RPC messages from stdin, POSTs each to
/// the MCP endpoint, and writes the response back to stdout.
///
/// The MCP Streamable HTTP transport works as:
/// - POST with JSON-RPC body → response is either direct JSON or SSE stream
/// - For SSE, we forward events as they arrive
///
/// On daemon connection failure, attempts to restart the daemon and
/// re-register the workspace (up to `MAX_RECONNECT_ATTEMPTS` times).
///
/// Exits on stdin EOF or unrecoverable bridge error.
async fn run_stdio_bridge(
    initial_mcp_url: &str,
    port: u16,
    workspace_root: &std::path::Path,
) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let client = reqwest::Client::builder()
        .build()
        .context("Failed to create HTTP client for bridge")?;

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    // Mutable state: URL and session can change on reconnect
    let mut mcp_url = initial_mcp_url.to_string();
    let mut session_id: Option<String> = None;
    let mut reconnect_attempts: u32 = 0;

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .context("Failed to read from stdin")?;

        if bytes_read == 0 {
            info!("stdin EOF, bridge shutting down");
            if let Some(ref sid) = session_id {
                debug!("Sending session teardown DELETE for session {}", sid);
                let _ = client
                    .delete(&mcp_url)
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

        // Extract the JSON-RPC request id so error responses can reference it.
        // This is a lightweight parse — we only need the "id" field.
        let request_id = serde_json::from_str::<serde_json::Value>(trimmed)
            .ok()
            .and_then(|v| v.get("id").cloned());

        // Send request with session recovery (retries once on 401 stale session),
        // falling back to daemon reconnect on TCP connection failure.
        let resp = match send_with_session_recovery(&client, &mcp_url, &mut session_id, trimmed)
            .await
        {
            Ok(r) => {
                reconnect_attempts = 0;
                r
            }
            Err(e) if e.is_connect() && reconnect_attempts < MAX_RECONNECT_ATTEMPTS => {
                reconnect_attempts += 1;
                warn!(
                    "Daemon connection lost (attempt {}/{}), reconnecting...",
                    reconnect_attempts, MAX_RECONNECT_ATTEMPTS
                );
                match attempt_reconnect(port, workspace_root).await {
                    Ok(new_url) => {
                        mcp_url = new_url;
                        session_id = None;
                        match send_to_daemon(&client, &mcp_url, &session_id, trimmed).await {
                            Ok(r) => {
                                reconnect_attempts = 0;
                                r
                            }
                            Err(e2) => {
                                write_jsonrpc_error(&mut stdout, request_id.as_ref(), &e2).await?;
                                continue;
                            }
                        }
                    }
                    Err(re) => {
                        error!("Reconnect failed: {}", re);
                        write_jsonrpc_error(&mut stdout, request_id.as_ref(), &e).await?;
                        continue;
                    }
                }
            }
            Err(e) => {
                write_jsonrpc_error(&mut stdout, request_id.as_ref(), &e).await?;
                continue;
            }
        };

        // Guard: don't forward raw HTTP error bodies for session failures.
        // These aren't valid JSON-RPC and would corrupt the MCP client's stream.
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED
            || resp.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY
        {
            warn!(
                "Unrecoverable session error ({}), returning JSON-RPC error to client",
                resp.status()
            );
            session_id = None;
            write_jsonrpc_error(
                &mut stdout,
                request_id.as_ref(),
                &format_args!("MCP session error (HTTP {})", resp.status().as_u16()),
            )
            .await?;
            continue;
        }

        if !resp.status().is_success() {
            warn!("Daemon returned HTTP {} for bridge request", resp.status());
        }

        // Update session ID from response headers (picks up new sessions after restart)
        if let Some(sid) = resp.headers().get("mcp-session-id") {
            if let Ok(sid_str) = sid.to_str() {
                if session_id.as_deref() != Some(sid_str) {
                    debug!("Updated MCP session ID: {}", sid_str);
                    session_id = Some(sid_str.to_string());
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
            if let Err(e) = forward_sse_stream(resp, &mut stdout).await {
                write_jsonrpc_error(&mut stdout, request_id.as_ref(), &e).await?;
                continue;
            }
        } else {
            // Direct JSON response — forward to stdout
            let body = resp.bytes().await.context("Failed to read response body")?;
            if !body.is_empty() {
                debug!("Bridge ← daemon: {}", String::from_utf8_lossy(&body));
                stdout.write_all(&body).await?;
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

/// Send a request to the daemon with automatic session recovery.
///
/// On HTTP 401 (stale session): clears the session and retries once without it.
/// On HTTP 422 or persistent 401: returns the error response as-is for the
/// caller to handle (typically by writing a clean JSON-RPC error).
///
/// This prevents the 401→422 death spiral where a stale session causes every
/// subsequent request to fail permanently.
pub(crate) async fn send_with_session_recovery(
    client: &reqwest::Client,
    mcp_url: &str,
    session_id: &mut Option<String>,
    body: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    let resp = send_to_daemon(client, mcp_url, session_id, body).await?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED && session_id.is_some() {
        warn!("Stale MCP session (401), clearing and retrying");
        *session_id = None;
        send_to_daemon(client, mcp_url, session_id, body).await
    } else {
        Ok(resp)
    }
}

/// Build and send a POST request to the daemon's MCP endpoint.
async fn send_to_daemon(
    client: &reqwest::Client,
    mcp_url: &str,
    session_id: &Option<String>,
    body: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut req = client
        .post(mcp_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream");
    if let Some(sid) = session_id {
        req = req.header("Mcp-Session-Id", sid);
    }
    req.body(body.to_string()).send().await
}

/// Attempt to restart the daemon and re-register the workspace.
/// Returns the new MCP URL on success.
async fn attempt_reconnect(port: u16, workspace_root: &std::path::Path) -> Result<String> {
    let daemon_port = ensure_daemon_running(port)
        .await
        .context("Failed to restart daemon during reconnect")?;
    let workspace_id = register_workspace(daemon_port, workspace_root)
        .await
        .context("Failed to re-register workspace during reconnect")?;
    let new_url = format!("http://localhost:{}/mcp/{}", daemon_port, workspace_id);
    info!("Reconnected to daemon: {}", new_url);
    Ok(new_url)
}

/// Write a JSON-RPC error response to stdout so the MCP client knows the request failed.
///
/// `id` is the request ID extracted from the incoming JSON-RPC request so the
/// error response can be correlated by the client. Falls back to `null` when
/// the ID could not be determined.
async fn write_jsonrpc_error(
    stdout: &mut tokio::io::Stdout,
    id: Option<&serde_json::Value>,
    err: &dyn std::fmt::Display,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    error!("Bridge HTTP error: {}", err);
    let id_value = id.unwrap_or(&serde_json::Value::Null);
    let error_resp = serde_json::json!({
        "jsonrpc": "2.0",
        "error": { "code": -32000, "message": format!("Bridge error: {}", err) },
        "id": id_value
    });
    let mut out = serde_json::to_vec(&error_resp)?;
    out.push(b'\n');
    stdout.write_all(&out).await?;
    stdout.flush().await?;
    Ok(())
}

/// Forward an SSE response stream to stdout, extracting `data:` lines as JSON-RPC messages.
async fn forward_sse_stream(
    resp: reqwest::Response,
    stdout: &mut tokio::io::Stdout,
) -> Result<()> {
    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;

    debug!("SSE response stream");
    let mut stream = resp.bytes_stream();
    let mut sse_buffer = String::new();

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                sse_buffer.push_str(&text);

                // Guard against unbounded buffer growth (e.g., malformed SSE without delimiters)
                if sse_buffer.len() > MAX_SSE_BUFFER_SIZE {
                    warn!("SSE buffer exceeded {}B without complete event, dropping", MAX_SSE_BUFFER_SIZE);
                    sse_buffer.clear();
                    continue;
                }

                // Process complete SSE events (delimited by blank lines)
                while let Some(event_end) = sse_buffer.find("\n\n") {
                    let event = sse_buffer[..event_end].to_string();
                    sse_buffer = sse_buffer[event_end + 2..].to_string();

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
                return Err(anyhow::anyhow!("SSE stream error: {}", e));
            }
        }
    }

    Ok(())
}
