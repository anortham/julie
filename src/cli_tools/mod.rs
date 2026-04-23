//! CLI tool surface for Julie.
//!
//! Provides shell-first access to Julie's code intelligence tools,
//! with named wrappers for high-frequency commands and a generic
//! fallback for any tool by name.
//!
//! ## Architecture
//!
//! `run_cli_tool` is the single entry point for all CLI tool invocations.
//! It supports two modes:
//!
//! - **Daemon mode** (default): connects to a running daemon via IPC, sends
//!   a JSON-RPC `tools/call` request, and returns the result.
//! - **Standalone mode** (`--standalone`): creates a local handler, indexes
//!   the workspace, and executes the tool in-process.
//!
//! If daemon connection fails (and `--standalone` was not specified), the
//! execution core falls back to standalone mode with a stderr warning.

pub mod commands;
pub mod daemon;
pub mod generic;
pub mod output;
pub mod subcommands;

pub use subcommands::*;

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use crate::cli::resolve_workspace_root;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;

use self::daemon::{DaemonCallError, DaemonClient};

// ---------------------------------------------------------------------------
// Execution mode tracking
// ---------------------------------------------------------------------------

/// Which execution mode the CLI tool ran in. Reported on stderr for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliExecutionMode {
    /// Connected to the daemon over IPC.
    Daemon,
    /// Running standalone with a local handler.
    Standalone,
    /// Daemon connection failed; fell back to standalone.
    DaemonFallback,
}

impl std::fmt::Display for CliExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliExecutionMode::Daemon => write!(f, "daemon"),
            CliExecutionMode::Standalone => write!(f, "standalone"),
            CliExecutionMode::DaemonFallback => write!(f, "standalone (daemon unavailable)"),
        }
    }
}

// ---------------------------------------------------------------------------
// CLI tool output
// ---------------------------------------------------------------------------

/// Result of a CLI tool execution, carrying the mode used and the raw
/// tool result for formatting by A4.
#[derive(Debug)]
pub struct CliToolOutput {
    /// Which mode was used for execution.
    pub mode: CliExecutionMode,
    /// The workspace root that was resolved.
    pub workspace_root: PathBuf,
    /// The raw result from the tool call, serialized as JSON.
    pub result: Value,
    /// Whether the tool indicated an error (isError field in CallToolResult).
    pub is_error: bool,
}

// ---------------------------------------------------------------------------
// CliToolCommand trait
// ---------------------------------------------------------------------------

/// Trait that each CLI tool command implements to bridge CLI args into
/// tool execution. A3 implements this for each named subcommand.
///
/// The trait provides two paths:
/// - `tool_name()` + `to_tool_args()` for daemon mode (JSON-RPC dispatch)
/// - `call_standalone()` for standalone mode (direct handler call)
#[async_trait]
pub trait CliToolCommand: Send + Sync {
    /// The MCP tool name (e.g. "fast_search", "fast_refs", "get_symbols").
    fn tool_name(&self) -> &'static str;

    /// Convert CLI args to JSON tool parameters for daemon-mode dispatch.
    fn to_tool_args(&self) -> Result<Value>;

    /// Validate that the command can run in standalone mode.
    ///
    /// Most commands support standalone execution. Commands that rely on
    /// daemon-only registry state should override this and return a clear
    /// actionable error.
    fn validate_standalone(&self) -> Result<()> {
        Ok(())
    }

    /// Execute the tool directly against a handler in standalone mode.
    async fn call_standalone(&self, handler: &JulieServerHandler) -> Result<CallToolResult>;
}

// ---------------------------------------------------------------------------
// Execution core
// ---------------------------------------------------------------------------

/// Execute a CLI tool command.
///
/// This is the single entry point for all tool subcommands. It resolves the
/// workspace, picks daemon or standalone mode, executes the tool, and returns
/// the result for formatting.
///
/// `command` implements `CliToolCommand` (A3 wires each subcommand's args).
/// `cli_workspace` is the `--workspace` flag from the CLI, if any.
/// `standalone` is true if `--standalone` was passed.
pub async fn run_cli_tool(
    command: &dyn CliToolCommand,
    cli_workspace: Option<PathBuf>,
    standalone: bool,
) -> Result<CliToolOutput> {
    if standalone {
        command.validate_standalone()?;
    }

    let start = Instant::now();
    let workspace_root = resolve_workspace_root(cli_workspace.clone());

    eprintln!("julie: workspace {}", workspace_root.display());

    let (mode, result_value, is_error) = if standalone {
        let result = run_standalone(command, &workspace_root).await?;
        let (value, is_err) = serialize_call_tool_result(result)?;
        (CliExecutionMode::Standalone, value, is_err)
    } else {
        match run_via_daemon(command, cli_workspace.clone()).await {
            Ok((value, is_err)) => (CliExecutionMode::Daemon, value, is_err),
            Err(daemon_err) => {
                if let Err(standalone_err) = command.validate_standalone() {
                    eprintln!(
                        "julie: daemon unavailable ({})",
                        summarize_error(&daemon_err)
                    );
                    return Err(standalone_err.context(format!(
                        "Daemon unavailable: {}",
                        summarize_error(&daemon_err)
                    )));
                }

                eprintln!(
                    "julie: daemon unavailable ({}), falling back to standalone mode",
                    summarize_error(&daemon_err)
                );
                let result = run_standalone(command, &workspace_root).await?;
                let (value, is_err) = serialize_call_tool_result(result)?;
                (CliExecutionMode::DaemonFallback, value, is_err)
            }
        }
    };

    let elapsed = start.elapsed();
    eprintln!(
        "julie: mode={}, elapsed={:.1}s",
        mode,
        elapsed.as_secs_f64()
    );

    Ok(CliToolOutput {
        mode,
        workspace_root,
        result: result_value,
        is_error,
    })
}

// ---------------------------------------------------------------------------
// Daemon mode
// ---------------------------------------------------------------------------

/// Execute a tool via daemon IPC.
///
/// Ensures the daemon is running, connects, sends the tool call, and returns
/// the result. Only transport-level failures (connection refused, timeout,
/// I/O errors) are returned as `Err` to allow standalone fallback. Tool-level
/// errors from the daemon (invalid params, workspace not found) are surfaced
/// as `Ok((value, true))` so the caller exits with the error instead of
/// silently retrying in standalone mode.
async fn run_via_daemon(
    command: &dyn CliToolCommand,
    cli_workspace: Option<PathBuf>,
) -> Result<(Value, bool)> {
    let tool_name = command.tool_name();
    let arguments = command.to_tool_args()?;

    daemon::ensure_daemon_ready()?;

    let startup_hint = build_cli_startup_hint(cli_workspace);
    let mut client = DaemonClient::connect(&startup_hint).await?;

    match client.call_tool(tool_name, arguments).await {
        Ok(result) => {
            let is_error = result
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Ok((result, is_error))
        }
        Err(DaemonCallError::ToolError { message, raw }) => {
            // The daemon processed the request and returned an error.
            // Surface it as a successful daemon call with is_error=true so
            // the CLI prints the error and exits 1 (no standalone fallback).
            let error_detail = raw
                .get("data")
                .map(|d| format!("\n{}", d))
                .unwrap_or_default();
            let error_value = serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": format!("{}{}", message, error_detail),
                }],
                "isError": true,
            });
            Ok((error_value, true))
        }
        Err(DaemonCallError::Transport(e)) => {
            // Transport failure: connection refused, handshake timeout, etc.
            // The caller should fall back to standalone mode.
            Err(e.context("Tool call via daemon failed"))
        }
    }
}

// ---------------------------------------------------------------------------
// Standalone mode
// ---------------------------------------------------------------------------

/// Bootstrap a standalone handler with an indexed workspace.
///
/// This is the shared infrastructure for standalone tool execution. It creates
/// a `JulieServerHandler`, validates the workspace path, and ensures the
/// workspace is indexed before returning the handler.
///
/// Exposed as `pub` so A3 tool wrappers can use it for testing or custom
/// standalone flows, though the normal path goes through `run_standalone`.
pub async fn bootstrap_standalone_handler(
    workspace_root: &std::path::Path,
) -> Result<JulieServerHandler> {
    if !workspace_root.exists() {
        anyhow::bail!(
            "Workspace path does not exist: {}\n\
             Specify a valid workspace with --workspace <path> or run from a project directory.",
            workspace_root.display()
        );
    }

    let handler = JulieServerHandler::new(workspace_root.to_path_buf())
        .await
        .context("Failed to create standalone handler")?;

    let julie_dir = workspace_root.join(".julie");
    if !julie_dir.exists() {
        eprintln!(
            "julie: workspace not indexed at {}\n\
             Indexing now (first run may take a moment)...",
            workspace_root.display()
        );
    }

    handler
        .initialize_workspace_with_force(None, false)
        .await
        .context("Failed to initialize workspace")?;

    // In standalone mode, initialize_workspace_with_force opens the workspace
    // handles, while run_auto_indexing normally fills SQLite and Tantivy after
    // the MCP on_initialized callback. CLI mode has no callback, so run the
    // index path here and skip embeddings to keep startup responsive.
    let has_workspace = handler.workspace.read().await.is_some();
    if has_workspace {
        let index_tool = crate::tools::workspace::commands::ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_root.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool
            .call_tool_with_options(&handler, true)
            .await
            .context("Failed to index standalone workspace")?;
    } else {
        anyhow::bail!(
            "Workspace not indexed: {}\n\
             Run `julie-server workspace index` or use daemon mode for automatic indexing.\n\
             If the workspace has no source files, there is nothing to index.",
            workspace_root.display()
        );
    }

    Ok(handler)
}

/// Execute a tool in standalone mode with a local handler.
async fn run_standalone(
    command: &dyn CliToolCommand,
    workspace_root: &std::path::Path,
) -> Result<CallToolResult> {
    let handler = bootstrap_standalone_handler(workspace_root).await?;
    command.call_standalone(&handler).await
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Serialize a `CallToolResult` into a JSON `Value` and extract the isError flag.
pub fn serialize_call_tool_result(result: CallToolResult) -> Result<(Value, bool)> {
    let is_error = result.is_error.unwrap_or(false);
    let value = serde_json::to_value(&result).context("Failed to serialize tool result")?;
    Ok((value, is_error))
}

/// Build a `WorkspaceStartupHint` from CLI arguments.
fn build_cli_startup_hint(
    cli_workspace: Option<PathBuf>,
) -> crate::workspace::startup_hint::WorkspaceStartupHint {
    let workspace_root = resolve_workspace_root(cli_workspace);
    daemon::build_startup_hint(workspace_root)
}

/// Summarize an error chain into a short single-line message for stderr.
fn summarize_error(err: &anyhow::Error) -> String {
    let root = err.root_cause();
    let msg = root.to_string();
    if msg.len() > 120 {
        format!("{}...", &msg[..117])
    } else {
        msg
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_execution_mode_display() {
        assert_eq!(CliExecutionMode::Daemon.to_string(), "daemon");
        assert_eq!(CliExecutionMode::Standalone.to_string(), "standalone");
        assert_eq!(
            CliExecutionMode::DaemonFallback.to_string(),
            "standalone (daemon unavailable)"
        );
    }

    #[test]
    fn test_summarize_error_short() {
        let err = anyhow::anyhow!("connection refused");
        assert_eq!(summarize_error(&err), "connection refused");
    }

    #[test]
    fn test_summarize_error_truncation() {
        let long_msg = "x".repeat(200);
        let err = anyhow::anyhow!("{}", long_msg);
        let summary = summarize_error(&err);
        assert!(summary.len() <= 120);
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn test_serialize_call_tool_result_success() {
        use crate::mcp_compat::Content;
        let result = CallToolResult::success(vec![Content::text("hello world")]);
        let (value, is_error) = serialize_call_tool_result(result).unwrap();
        assert!(!is_error);
        assert!(value.get("content").is_some());
    }

    #[test]
    fn test_serialize_call_tool_result_error() {
        use crate::mcp_compat::Content;
        let result = CallToolResult::error(vec![Content::text("something went wrong")]);
        let (_, is_error) = serialize_call_tool_result(result).unwrap();
        assert!(is_error);
    }
}
