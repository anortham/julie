//! Adapter: the thin process that MCP clients spawn.
//!
//! The adapter auto-starts the daemon if not running, connects to the daemon's
//! canonical HTTP MCP transport, then forwards stdin/stdout JSON-RPC between
//! the MCP client and daemon. From the MCP client's perspective, it looks
//! exactly like a stdio MCP server.

pub mod launcher;

use anyhow::Result;

use crate::paths::DaemonPaths;
use crate::workspace::startup_hint::WorkspaceStartupHint;

mod forwarder;
mod http_stdio;

#[cfg(test)]
pub(crate) use forwarder::{AdapterError, forward_http_stdio_transport};
#[cfg(test)]
pub(crate) use http_stdio::{
    AdapterRetryDecision, DaemonAdapterControl, MAX_RETRIES, classify_adapter_error, retry_backoff,
    run_http_adapter_inner,
};

use self::http_stdio::run_http_adapter;
use self::launcher::DaemonLauncher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForwardOutcome {
    SessionEnded,
    ImmediateDaemonDisconnect,
}

/// Run the adapter: auto-start daemon, connect to HTTP MCP, forward messages.
pub async fn run_adapter(startup_hint: WorkspaceStartupHint) -> Result<()> {
    let paths = DaemonPaths::new();
    let launcher = DaemonLauncher::new(paths);

    run_http_adapter(startup_hint, launcher).await
}
