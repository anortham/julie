//! Symbol Overview Tools - Understand file structure without reading full content
//!
//! This module provides tools for getting symbol-level overviews of files,
//! similar to Serena's get_symbols_overview. This is essential for:
//! - Understanding file structure without wasting context on full reads
//! - Finding insertion points for new code
//! - Discovering available symbols before diving into details
//!
//! Unlike reading entire files with the Read tool, these tools provide
//! just the "skeleton" - symbol names, types, signatures, and locations.

pub mod body_extraction;
pub mod filtering;
pub mod formatting;
mod primary;
mod rows;
mod target_workspace;

use anyhow::{Result, bail};
use julie_core::mcp_compat::CallToolResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::navigation::resolution::WorkspaceTarget;
use julie_context::ToolContext;

fn default_max_depth() -> u32 {
    1
}

fn default_limit() -> Option<u32> {
    Some(50) // Default limit to prevent token overflow on large files
}

fn default_mode() -> Option<String> {
    Some("structure".to_string()) // Default to structure for token-efficient orientation
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

fn validated_mode(mode: Option<&str>) -> Result<&str> {
    let mode = mode.unwrap_or("structure");
    match mode {
        "structure" | "minimal" | "full" => Ok(mode),
        other => bail!(
            "Invalid mode: '{}'. Expected one of: structure, minimal, full",
            other
        ),
    }
}

fn file_not_found_message(file_path: &str, target: Option<&str>) -> String {
    let mut message = format!("❌ File not found: {}", file_path);
    if let Some(target_name) = target {
        message.push_str(&format!(
            "\n💡 Try deep_dive(symbol=\"{}\") to find it without needing the file path",
            target_name
        ));
    } else {
        let filename = std::path::Path::new(file_path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());
        message.push_str(&format!(
            "\n💡 Try fast_search(query=\"{}\") to locate the file",
            filename
        ));
    }
    message
}

//**********************//
//   Get Symbols Tool   //
//**********************//

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolsTool {
    /// File path (relative to workspace root)
    pub file_path: String,
    /// Maximum nesting depth (default: 1). 0=top-level, 1=include methods, 2+=deeper
    #[serde(
        default = "default_max_depth",
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub max_depth: u32,
    /// Filter to specific symbol name (supports partial matching). Combine with mode="minimal" to extract just one symbol's code (90% token savings vs reading the whole file)
    #[serde(default)]
    pub target: Option<String>,
    /// Maximum symbols to return (default: 50)
    #[serde(
        default = "default_limit",
        deserialize_with = "julie_core::serde_lenient::deserialize_option_u32_lenient"
    )]
    pub limit: Option<u32>,
    /// Skip this many symbols before keeping `limit` rows (default: 0)
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub offset: u32,
    /// Reading mode: "structure" (names/signatures only, no code), "minimal" (default, code bodies for top-level symbols), "full" (code for all including nested). WARNING: "full" without target extracts the entire file
    #[serde(default = "default_mode")]
    pub mode: Option<String>,
    /// Required workspace ID or absolute workspace path for MCP calls
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Zero-based source-line offset within each canonical symbol span.
    #[serde(default)]
    pub body_offset: u32,
    /// Maximum source lines returned for each body (default: 100).
    #[serde(default)]
    pub body_limit: Option<u32>,
    /// Source hash from the previous body page; rejects changed source.
    #[serde(default)]
    pub source_hash: Option<String>,
}

impl GetSymbolsTool {
    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        self.call_tool_counted(handler)
            .await
            .map(|(result, _)| result)
    }

    pub async fn call_tool_counted(
        &self,
        handler: &dyn ToolContext,
    ) -> Result<(CallToolResult, u32)> {
        let mode = validated_mode(self.mode.as_deref())?;

        let workspace_target = handler
            .resolve_workspace_target(self.workspace.as_deref())
            .await?;
        let mut next_request = self.clone();
        next_request.mode = Some(mode.to_string());
        next_request.workspace = Some(crate::shared::resolved_workspace(
            handler,
            &workspace_target,
        )?);

        match workspace_target {
            WorkspaceTarget::Target(target_workspace_id) => {
                debug!("🎯 Querying workspace: {}", target_workspace_id);
                target_workspace::get_symbols_from_target_workspace(
                    handler,
                    &self.file_path,
                    self.max_depth,
                    self.target.as_deref(),
                    self.limit,
                    self.offset,
                    mode,
                    target_workspace_id,
                    &next_request,
                )
                .await
            }
            WorkspaceTarget::Primary => {
                primary::get_symbols_from_primary(
                    handler,
                    &self.file_path,
                    self.max_depth,
                    self.target.as_deref(),
                    self.limit,
                    self.offset,
                    mode,
                    &next_request,
                )
                .await
            }
        }
    }
}
