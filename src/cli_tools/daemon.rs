//! Lightweight daemon HTTP client for CLI tool execution.
//!
//! Reuses the same daemon Streamable HTTP discovery and header contract as the
//! stdio adapter, then uses rmcp's client service to call tools.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rmcp::model::{CallToolRequestParams, ClientInfo, Implementation, JsonObject};
use rmcp::service::{RoleClient, RunningService, ServiceError};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{ServiceExt, model::CallToolResult};
use serde_json::Value;
use thiserror::Error;

use crate::daemon::http_client::http_client_config_for_endpoint;
use crate::daemon::transport::TransportEndpoint;
use crate::paths::DaemonPaths;
use crate::workspace::startup_hint::WorkspaceStartupHint;

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
/// Connects to the daemon HTTP MCP endpoint, lets rmcp perform initialization,
/// sends one `tools/call` request, reads the response, and disconnects.
pub struct DaemonClient {
    service: RunningService<RoleClient, ClientInfo>,
}

impl DaemonClient {
    /// Connect to the daemon and complete the HTTP MCP initialize handshake.
    ///
    /// Uses `DaemonPaths` and `WorkspaceStartupHint` to locate the HTTP
    /// endpoint and build the same headers the adapter sends.
    pub async fn connect(startup_hint: &WorkspaceStartupHint) -> Result<Self> {
        let paths = DaemonPaths::new();
        let discovery_path = paths.daemon_mcp_transport();
        let endpoint = TransportEndpoint::read_discovery(&discovery_path).with_context(|| {
            format!(
                "Failed to read daemon HTTP MCP discovery at {}",
                discovery_path.display()
            )
        })?;
        let config = http_client_config_for_endpoint(&endpoint, startup_hint)?;
        let transport = StreamableHttpClientTransport::from_config(config);
        let service = cli_client_info()
            .serve(transport)
            .await
            .context("Failed to initialize daemon HTTP MCP client")?;
        Ok(Self { service })
    }

    /// Send an MCP `tools/call` request and return the response.
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
        let arguments = tool_arguments_from_value(arguments)?;
        let result = self
            .service
            .call_tool(CallToolRequestParams::new(tool_name.to_string()).with_arguments(arguments))
            .await
            .map_err(map_call_tool_error)?;
        serialize_call_tool_result(result)
    }
}

fn cli_client_info() -> ClientInfo {
    let mut info = ClientInfo::default();
    info.client_info = Implementation::new("julie-cli", env!("CARGO_PKG_VERSION"));
    info
}

fn tool_arguments_from_value(arguments: Value) -> Result<JsonObject, DaemonCallError> {
    match arguments {
        Value::Object(arguments) => Ok(arguments),
        other => Err(DaemonCallError::Transport(anyhow::anyhow!(
            "Daemon tool arguments must be a JSON object, got {}",
            other_type_name(&other)
        ))),
    }
}

fn serialize_call_tool_result(result: CallToolResult) -> Result<Value, DaemonCallError> {
    serde_json::to_value(result).map_err(|error| {
        DaemonCallError::Transport(
            anyhow::Error::from(error).context("Failed to serialize daemon tool result"),
        )
    })
}

fn map_call_tool_error(error: ServiceError) -> DaemonCallError {
    match error {
        ServiceError::McpError(error) => {
            let message = error.message.to_string();
            let raw = serde_json::to_value(&error).unwrap_or_else(|_| {
                serde_json::json!({
                    "message": message,
                })
            });
            DaemonCallError::ToolError { message, raw }
        }
        other => DaemonCallError::Transport(anyhow::Error::from(other)),
    }
}

#[cfg(test)]
pub(crate) fn map_call_tool_error_for_test(error: ServiceError) -> DaemonCallError {
    map_call_tool_error(error)
}

fn other_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
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

/// Check whether a daemon appears to be running by probing HTTP discovery.
///
/// This is a quick readiness probe, not a full MCP connection. Used by the
/// execution core to decide whether to attempt daemon mode.
pub fn daemon_appears_running() -> bool {
    let paths = DaemonPaths::new();
    TransportEndpoint::read_discovery(&paths.daemon_mcp_transport())
        .map(|endpoint| endpoint.probe_readiness().is_ready())
        .unwrap_or(false)
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

/// The workspace root as resolved from CLI args, for daemon HTTP headers.
pub fn build_startup_hint(workspace_root: PathBuf) -> WorkspaceStartupHint {
    WorkspaceStartupHint {
        path: workspace_root,
        source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cli),
    }
}
