//! Lightweight daemon IPC client for CLI tool execution.
//!
//! Reuses the same IPC endpoint and ready gate as the adapter, then performs
//! the MCP initialize flow before sending a one-shot `tools/call` request.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::adapter::{ReadyOutcome, build_ipc_header, read_daemon_ready};
use crate::daemon::ipc::{IpcClientStream, IpcConnector};
use crate::paths::DaemonPaths;
use crate::workspace::startup_hint::WorkspaceStartupHint;

/// Timeout for the daemon's READY handshake signal during CLI invocations.
/// Shorter than the adapter's 30s since CLI users expect snappy responses.
const CLI_READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Errors from daemon tool calls. Distinguishes transport failures (where
/// standalone fallback is appropriate) from tool-level errors (where the
/// daemon processed the request and returned an error response).
#[derive(Debug, Error)]
pub enum DaemonCallError {
    /// Transport-level failure: connection refused, handshake timeout, I/O
    /// error, etc. Standalone fallback is appropriate.
    #[error("{0}")]
    Transport(#[from] anyhow::Error),

    /// The daemon received and processed the request but returned a JSON-RPC
    /// error response. Fallback would be wrong; surface this to the user.
    #[error("Daemon tool error: {message}")]
    ToolError {
        message: String,
        /// The raw JSON-RPC error object for structured access.
        raw: Value,
    },
}

/// A single-shot daemon client for CLI tool calls.
///
/// Connects to the daemon IPC endpoint, performs the handshake, sends one
/// JSON-RPC `tools/call` request, reads the response, and disconnects.
pub struct DaemonClient {
    stream: IpcClientStream,
    mcp_initialized: bool,
}

impl DaemonClient {
    #[cfg(test)]
    pub(crate) fn from_stream_for_test(stream: IpcClientStream) -> Self {
        Self {
            stream,
            mcp_initialized: false,
        }
    }

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
            ReadyOutcome::Ready => Ok(Self {
                stream,
                mcp_initialized: false,
            }),
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
    /// The daemon's session handler is a full MCP server, so the CLI must run
    /// `initialize` before the request instead of sending `tools/call` as the
    /// first MCP message on the stream.
    ///
    /// Returns `DaemonCallError::ToolError` when the daemon processes the
    /// request but returns a JSON-RPC error (invalid params, workspace not
    /// found, etc.). Returns `DaemonCallError::Transport` for I/O and
    /// protocol-level failures where standalone fallback is appropriate.
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, DaemonCallError> {
        self.ensure_mcp_initialized().await?;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        self.write_json_message(&request, "tool call request")
            .await?;
        let response = self.read_response_with_id(1, "tool call response").await?;

        // Extract the result or error from the JSON-RPC envelope.
        // A JSON-RPC error means the daemon processed the request and
        // rejected it (invalid params, tool error, etc.). This is NOT a
        // transport failure, so standalone fallback would be wrong.
        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error")
                .to_string();
            return Err(DaemonCallError::ToolError {
                message,
                raw: error.clone(),
            });
        }

        response.get("result").cloned().ok_or_else(|| {
            DaemonCallError::Transport(anyhow::anyhow!("Daemon response missing 'result' field"))
        })
    }

    async fn ensure_mcp_initialized(&mut self) -> Result<(), DaemonCallError> {
        if self.mcp_initialized {
            return Ok(());
        }

        let initialize = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {
                    "name": "julie-cli",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });
        self.write_json_message(&initialize, "initialize request")
            .await?;
        let response = self.read_response_with_id(0, "initialize response").await?;
        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(DaemonCallError::Transport(anyhow::anyhow!(
                "Daemon initialize error: {message}"
            )));
        }
        if response.get("result").is_none() {
            return Err(DaemonCallError::Transport(anyhow::anyhow!(
                "Daemon initialize response missing 'result' field"
            )));
        }

        let initialized = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        self.write_json_message(&initialized, "initialized notification")
            .await?;
        self.mcp_initialized = true;
        Ok(())
    }

    async fn write_json_message(
        &mut self,
        value: &Value,
        label: &str,
    ) -> Result<(), DaemonCallError> {
        let mut bytes =
            serde_json::to_vec(value).with_context(|| format!("Failed to serialize {label}"))?;
        bytes.push(b'\n');
        self.stream
            .write_all(&bytes)
            .await
            .with_context(|| format!("Failed to send {label} to daemon"))?;
        Ok(())
    }

    async fn read_response_with_id(
        &mut self,
        request_id: u64,
        label: &str,
    ) -> Result<Value, DaemonCallError> {
        loop {
            let response = self.read_json_line(label).await?;
            let Some(id) = response.get("id") else {
                continue;
            };
            if id == request_id {
                return Ok(response);
            }
        }
    }

    async fn read_json_line(&mut self, label: &str) -> Result<Value, DaemonCallError> {
        let mut response_bytes = Vec::new();
        let mut byte = [0u8; 1];

        loop {
            match self.stream.read(&mut byte).await {
                Ok(0) if response_bytes.is_empty() => {
                    return Err(DaemonCallError::Transport(anyhow::anyhow!(
                        "Daemon closed connection without sending a response"
                    )));
                }
                Ok(0) => {
                    return Err(DaemonCallError::Transport(anyhow::anyhow!(
                        "Daemon closed connection during {label}"
                    )));
                }
                Ok(_) if byte[0] == b'\n' => break,
                Ok(_) => response_bytes.push(byte[0]),
                Err(error) => {
                    return Err(DaemonCallError::Transport(
                        anyhow::Error::from(error)
                            .context(format!("Failed to read {label} from daemon")),
                    ));
                }
            }
        }

        serde_json::from_slice(&response_bytes).map_err(|error| {
            DaemonCallError::Transport(anyhow::Error::from(error).context(format!(
                "Failed to parse daemon {label} as JSON (first 200 bytes): {}",
                String::from_utf8_lossy(&response_bytes[..response_bytes.len().min(200)])
            )))
        })
    }
}

/// Attempt to connect to the daemon, returning None on connection failure.
///
/// This is used by the fallback logic: if daemon connection fails and the
/// user didn't explicitly request `--standalone`, we fall back to standalone
/// mode with a stderr warning instead of hard-failing.
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
