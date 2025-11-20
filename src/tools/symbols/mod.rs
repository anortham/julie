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

mod body_extraction;
mod filtering;
mod formatting;
mod primary;
mod reference;

use anyhow::Result;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::schema::CallToolResult;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::tools::navigation::resolution::resolve_workspace_filter;

fn default_max_depth() -> u32 {
    1
}

fn default_limit() -> Option<u32> {
    Some(50) // Default limit to prevent token overflow on large files
}

fn default_mode() -> Option<String> {
    Some("structure".to_string())
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

//**********************//
//   Get Symbols Tool   //
//**********************//

#[mcp_tool(
    name = "get_symbols",
    description = "Get file structure and symbols with optional body extraction.",
    title = "Get File Symbols",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "performance": "instant", "agent_hint": "structure_first_then_targeted_bodies"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolsTool {
    /// File path (relative to workspace root)
    pub file_path: String,
    /// Maximum nesting depth (default: 1). 0=top-level, 1=include methods, 2+=deeper
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Filter to specific symbol name (supports partial matching)
    #[serde(default)]
    pub target: Option<String>,
    /// Maximum symbols to return (default: 50)
    #[serde(default = "default_limit")]
    pub limit: Option<u32>,
    /// Reading mode: "structure" (default), "minimal", or "full"
    #[serde(default = "default_mode")]
    pub mode: Option<String>,
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Output format: "json" (default), "toon", or "auto" (TOON for 5+ symbols)
    #[serde(default)]
    pub output_format: Option<String>,
}

impl GetSymbolsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Resolve workspace parameter (primary vs reference workspace)
        let workspace_filter = resolve_workspace_filter(self.workspace.as_deref(), handler).await?;

        // If reference workspace is specified, handle it separately
        if let Some(ref_workspace_id) = workspace_filter {
            debug!("🎯 Querying reference workspace: {}", ref_workspace_id);
            return reference::get_symbols_from_reference(
                handler,
                &self.file_path,
                self.max_depth,
                self.target.as_deref(),
                self.limit,
                self.mode.as_deref().unwrap_or("structure"),
                ref_workspace_id,
                self.output_format.as_deref(),
            )
            .await;
        }

        // Primary workspace logic
        primary::get_symbols_from_primary(
            handler,
            &self.file_path,
            self.max_depth,
            self.target.as_deref(),
            self.limit,
            self.mode.as_deref().unwrap_or("structure"),
            self.output_format.as_deref(),
        )
        .await
    }
}
