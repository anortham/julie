//! Lightweight daemon IPC client for CLI tool execution.
//!
//! Reuses the same IPC endpoint and handshake protocol as the adapter,
//! but instead of forwarding a full stdio session, sends a single JSON-RPC
//! `tools/call` request and reads back the response.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::adapter::{ReadyOutcome, build_ipc_header, read_daemon_ready};
use crate::daemon::ipc::{IpcClientStream, IpcConnector};
use crate::paths::DaemonPaths;
use crate::workspace::startup_hint::WorkspaceStartupHint;

/// Timeout for the daemon's READY handshake signal during CLI invocations.
/// Shorter than the adapter's 30s since CLI users expect snappy responses.
const CLI_READY_TIMEOUT: Duration = Duration::from_secs(10);

/// A single-shot daemon client for CLI tool calls.
///
/// Connects to the daemon IPC endpoint, performs the handshake, sends one
/// JSON-RPC `tools/call` request, reads the response, and disconnects.
pub struct DaemonClient {
    stream: IpcClientStream,
}

impl DaemonClient {
    /// Connect to the daemon and complete the IPC handshake.
    ///
    /// Uses `DaemonPaths` and `WorkspaceStartupHint` to locate the IPC
    /// endpoint and build the header, same as the adapter does.
    pub async fn connect(startup_hint: &WorkspaceStartupHint) -> Result<Self> {
        let paths = DaemonPaths::new();
        let ipc_addr = paths.daemon_ipc_addr();

        let mut stream = IpcConnector::connect(&ipc_addr).await.with_context(|| {
            format!("Failed to connect to daemon IPC at {}", ipc_addr.display())
        })?;

        // Send the same IPC header the adapter sends
        let header = build_ipc_header(startup_hint);
        stream
            .write_all(header.as_bytes())
            .await
            .context("Failed to send IPC headers to daemon")?;

        // Wait for READY signal
        match read_daemon_ready(&mut stream, CLI_READY_TIMEOUT).await {
            ReadyOutcome::Ready => Ok(Self { stream }),
            ReadyOutcome::Eof => {
                anyhow::bail!("Daemon closed connection before ready signal (may be restarting)")
            }
            ReadyOutcome::Timeout => anyhow::bail!(
                "Daemon did not respond within {:?} (may be overloaded or stale)",
                CLI_READY_TIMEOUT
            ),
            ReadyOutcome::Unexpected(line) => anyhow::bail!(
                "Daemon sent unexpected handshake line: {:?}",
                String::from_utf8_lossy(&line)
            ),
            ReadyOutcome::IoError(e) => {
                Err(anyhow::Error::from(e)).context("I/O error during daemon handshake")
            }
        }
    }

    /// Send a JSON-RPC `tools/call` request and return the response.
    ///
    /// The request follows MCP's JSON-RPC format. The daemon's session handler
    /// processes it and returns a `tools/call` response with `CallToolResult`.
    pub async fn call_tool(&mut self, tool_name: &str, arguments: Value) -> Result<Value> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        let mut request_bytes =
            serde_json::to_vec(&request).context("Failed to serialize tool call request")?;
        request_bytes.push(b'\n');

        self.stream
            .write_all(&request_bytes)
            .await
            .context("Failed to send tool call request to daemon")?;

        // Read response line. The daemon sends JSON-RPC responses as
        // newline-delimited JSON over the IPC stream.
        let mut reader = BufReader::new(&mut self.stream);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .await
            .context("Failed to read tool call response from daemon")?;

        if response_line.is_empty() {
            anyhow::bail!("Daemon closed connection without sending a response");
        }

        let response: Value = serde_json::from_str(&response_line).with_context(|| {
            format!(
                "Failed to parse daemon response as JSON (first 200 chars): {}",
                &response_line[..response_line.len().min(200)]
            )
        })?;

        // Extract the result or error from the JSON-RPC envelope
        if let Some(error) = response.get("error") {
            anyhow::bail!("Daemon returned error: {}", error);
        }

        response
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Daemon response missing 'result' field"))
    }
}

/// Attempt to connect to the daemon, returning None on connection failure.
///
/// This is used by the fallback logic: if daemon connection fails and the
/// user didn't explicitly request `--standalone`, we fall back to standalone
/// mode with a stderr warning rather than hard-failing.
pub async fn try_connect_daemon(startup_hint: &WorkspaceStartupHint) -> Option<DaemonClient> {
    DaemonClient::connect(startup_hint).await.ok()
}

/// Check whether a daemon appears to be running by probing the IPC socket.
///
/// This is a quick filesystem check, not a full connection. Used by the
/// execution core to decide whether to attempt daemon mode.
pub fn daemon_appears_running() -> bool {
    let paths = DaemonPaths::new();
    let ipc_addr = paths.daemon_ipc_addr();

    #[cfg(unix)]
    {
        ipc_addr.exists()
    }

    #[cfg(windows)]
    {
        // On Windows, named pipes don't show up on the filesystem.
        // Try a synchronous probe.
        std::fs::metadata(&ipc_addr).is_ok()
    }
}

/// Ensure the daemon is running before attempting to connect.
///
/// Reuses the adapter's `DaemonLauncher` to spawn or wait for the daemon.
/// Returns Ok(()) if the daemon is ready, Err if it cannot be started.
pub fn ensure_daemon_ready() -> Result<()> {
    let paths = DaemonPaths::new();
    let launcher = crate::adapter::launcher::DaemonLauncher::new(paths);
    launcher
        .ensure_daemon_ready()
        .context("Failed to ensure daemon is ready for CLI tool call")
}

/// The workspace root as resolved from CLI args, for IPC header construction.
pub fn build_startup_hint(workspace_root: PathBuf) -> WorkspaceStartupHint {
    WorkspaceStartupHint {
        path: workspace_root,
        source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cli),
    }
}
