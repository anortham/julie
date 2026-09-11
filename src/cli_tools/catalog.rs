//! CLI catalog discovery: listing tools and printing schemas.
//!
//! Provides zero-warmup discovery: uses static metadata and schemars to inspect
//! tools and parameters without model loading, background workers, or workspace indexing.

use crate::cli_tools::subcommands::OutputFormat;
use crate::request_engine::{RequestFailure, ToolCatalog, ToolInfo};
use serde_json::Value;

/// Returns all tools with schemas and descriptions.
pub fn list_tools() -> Vec<ToolInfo> {
    ToolCatalog::list()
}

/// Returns the JSON Schema for the specified tool.
pub fn get_tool_schema(name: &str) -> Result<Value, RequestFailure> {
    ToolCatalog::schema(name)
}

/// Execute `tools list` with the requested output format.
pub fn run_tools_list(format: OutputFormat) -> Result<(String, i32), RequestFailure> {
    match format {
        OutputFormat::Json => {
            let tools = list_tools();
            let json_str = serde_json::to_string_pretty(&tools)
                .map_err(|e| RequestFailure::internal(format!("Serialization error: {e}")))?;
            Ok((json_str, 0))
        }
        OutputFormat::Text | OutputFormat::Markdown => {
            let tools = list_tools();
            let mut out = format!(
                "Julie Code Intelligence Tools ({}):\n\n",
                crate::request_engine::catalog::AVAILABLE_TOOLS.len()
            );
            for tool in tools {
                out.push_str(&format!("- {:<18} {}\n", tool.name, tool.description));
            }
            Ok((out, 0))
        }
    }
}

/// Execute `tools schema <name>` with the requested output format.
pub fn run_tools_schema(name: &str, format: OutputFormat) -> Result<(String, i32), RequestFailure> {
    let schema = get_tool_schema(name)?;
    match format {
        OutputFormat::Json => {
            let json_str = serde_json::to_string_pretty(&schema)
                .map_err(|e| RequestFailure::internal(format!("Serialization error: {e}")))?;
            Ok((json_str, 0))
        }
        OutputFormat::Text | OutputFormat::Markdown => {
            let json_str = serde_json::to_string_pretty(&schema)
                .map_err(|e| RequestFailure::internal(format!("Serialization error: {e}")))?;
            Ok((
                format!("# Schema for {name}\n\n```json\n{json_str}\n```"),
                0,
            ))
        }
    }
}
