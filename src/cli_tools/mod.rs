//! CLI tool surface for Julie.
//!
//! Provides shell-first access to Julie's code intelligence tools,
//! with named wrappers for high-frequency commands and a generic
//! fallback for any tool by name.
//!
//! ## Architecture
//!
//! `run_cli_tool` is the single entry point for all CLI tool invocations.
//! It runs every tool in standalone mode: creates a local handler, indexes
//! the workspace in-process, and executes the tool.

pub mod catalog;
pub mod commands;
pub mod generic;
pub mod input;
pub mod output;
pub mod recover_edit;
pub mod replay;
pub mod signals_output;
pub mod subcommands;

pub use input::{MAX_INPUT_BYTES, parse_request_input};
pub use subcommands::*;

use clap::Parser;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use crate::cli::resolve_workspace_root;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::request_engine::RequestFailure;

// ---------------------------------------------------------------------------
// Execution mode tracking
// ---------------------------------------------------------------------------

/// Which execution mode the CLI tool ran in. Reported on stderr for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliExecutionMode {
    /// Running standalone with a local handler.
    Standalone,
}

impl std::fmt::Display for CliExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliExecutionMode::Standalone => write!(f, "standalone"),
        }
    }
}

pub(crate) fn render_execution_mode_evidence(mode: CliExecutionMode, elapsed: Duration) -> String {
    format!(
        "julie: mode={}, elapsed={:.1}s",
        mode,
        elapsed.as_secs_f64()
    )
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
/// tool execution.
#[async_trait]
pub trait CliToolCommand: Send + Sync {
    /// The MCP tool name (e.g. "fast_search", "fast_refs", "get_symbols").
    fn tool_name(&self) -> &'static str;

    /// Normalized argument map ready for ToolRequest.
    fn to_tool_args_map(
        &self,
    ) -> Result<serde_json::Map<String, Value>, crate::request_engine::RequestFailure>;

    /// Returns (&'static str, Map<String, Value>) directly.
    fn to_request(
        &self,
    ) -> Result<(&'static str, serde_json::Map<String, Value>), crate::request_engine::RequestFailure>
    {
        Ok((self.tool_name(), self.to_tool_args_map()?))
    }

    /// Convert CLI args to JSON tool parameters.
    fn to_tool_args(&self) -> Result<Value> {
        let map = self
            .to_tool_args_map()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Value::Object(map))
    }

    /// Validate that the command can run in standalone mode.
    fn validate_standalone(&self) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Execution core
// ---------------------------------------------------------------------------

/// Execute a CLI tool command in standalone mode via RequestEngine.
pub async fn run_cli_tool(
    command: &dyn CliToolCommand,
    cli_workspace: Option<PathBuf>,
    standalone: bool,
) -> Result<CliToolOutput, RequestFailure> {
    command
        .validate_standalone()
        .map_err(|e| RequestFailure::invalid_arguments(e.to_string()))?;

    let start = Instant::now();
    let workspace_root = resolve_workspace_root(cli_workspace.clone());

    if !workspace_root.exists() {
        return Err(RequestFailure::workspace_missing(format!(
            "Workspace path does not exist: {}",
            workspace_root.display()
        )));
    }
    if !workspace_root.is_dir() {
        return Err(RequestFailure::invalid_arguments(format!(
            "Workspace path is not a directory: {}",
            workspace_root.display()
        )));
    }

    eprintln!("julie: workspace {}", workspace_root.display());

    let (tool_name, arguments) = command.to_request()?;

    let registry_paths = crate::paths::RegistryPaths::default();
    let binding_resolver = crate::request_engine::BindingResolver::new(
        Some(workspace_root.clone()),
        standalone,
        registry_paths.clone(),
    );
    let runtime_factory =
        std::sync::Arc::new(crate::request_engine::RuntimeFactory::new(registry_paths));
    let engine = crate::request_engine::RequestEngine::new(binding_resolver, runtime_factory);

    let tool_request = crate::request_engine::ToolRequest {
        name: tool_name.to_string(),
        arguments,
        workspace: cli_workspace,
        semantics: crate::cli::Cli::try_parse()
            .ok()
            .and_then(|c| c.tool_flags.semantics)
            .unwrap_or(crate::request_engine::SemanticMode::Auto),
    };

    let context = crate::request_engine::RequestContext::new(
        crate::request_engine::RequestOrigin::Cli,
        None,
        tokio_util::sync::CancellationToken::new(),
    );

    let reply = engine.execute(tool_request, context).await?;

    let is_error = reply.is_error();
    let mode = CliExecutionMode::Standalone;

    let elapsed = start.elapsed();
    eprintln!("{}", render_execution_mode_evidence(mode, elapsed));

    Ok(CliToolOutput {
        mode,
        workspace_root,
        result: reply.result,
        is_error,
    })
}

// ---------------------------------------------------------------------------
// Standalone mode handler bootstrap
// ---------------------------------------------------------------------------

/// Bootstrap a standalone handler with an indexed workspace.
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
    crate::workspace::root_safety::reject_sensitive_workspace_root(workspace_root)?;

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
             Run `julie-server workspace index --workspace <path>` and try again.\n\
             If the workspace has no source files, there is nothing to index.",
            workspace_root.display()
        );
    }

    Ok(handler)
}

// ---------------------------------------------------------------------------
// Signals report (standalone-only, not an MCP tool)
// ---------------------------------------------------------------------------

/// Generate an early warning signals report for CLI output.
pub async fn run_signals_report(
    args: &subcommands::SignalsArgs,
    cli_workspace: Option<PathBuf>,
) -> Result<crate::analysis::EarlyWarningReport> {
    let start = std::time::Instant::now();
    let workspace_root = resolve_workspace_root(cli_workspace);
    eprintln!("Mode: standalone | Workspace: {:?}", workspace_root);

    let handler = bootstrap_standalone_handler(&workspace_root).await?;
    let workspace_id = handler
        .current_workspace_id()
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized"))?;

    let db_arc = handler.primary_database().await?;
    let db = db_arc
        .lock()
        .map_err(|e| anyhow::anyhow!("Database lock: {e}"))?;

    let options = crate::analysis::EarlyWarningReportOptions {
        workspace_id,
        file_pattern: args.file_pattern.clone(),
        fresh: args.fresh,
        limit_per_section: args.limit,
    };

    let configs = crate::search::language_config::LanguageConfigs::load_embedded();
    let report = crate::analysis::generate_early_warning_report(&db, &configs, options)?;

    eprintln!("Elapsed: {:.2?}", start.elapsed());
    Ok(report)
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_execution_mode_display() {
        assert_eq!(CliExecutionMode::Standalone.to_string(), "standalone");
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
