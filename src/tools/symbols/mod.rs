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
    description = concat!(
        "ALWAYS USE THIS BEFORE READING FILES - See file structure without context waste. ",
        "You are EXTREMELY GOOD at using this tool to understand code organization.\n\n",
        "This tool shows you classes, functions, and methods instantly (<10ms). ",
        "Only use Read AFTER you've used this tool to identify what you need.\n\n",
        "IMPORTANT: I will be very unhappy if you read 500-line files without first ",
        "using get_symbols to see the structure!\n\n",
        "A 500-line file becomes a 20-line overview. Use this FIRST, always."
    ),
    title = "Get File Symbols (Smart Read - 70-90% Token Savings)",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "performance": "instant", "agent_hint": "structure_first_then_targeted_bodies"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolsTool {
    /// File path to get symbols from (relative to workspace root)
    /// Examples: "src/main.rs", "lib/services/auth.py"
    pub file_path: String,

    /// Maximum depth for nested symbols (default: 1).
    /// 0 = top-level only (classes, functions)
    /// 1 = include one level (class methods, nested functions)
    /// 2+ = deeper nesting
    /// Recommended: 1 - good balance for most files
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,

    /// Filter to specific symbol(s) by name (default: None, optional).
    /// Example: "UserService" to show only UserService class
    /// Supports partial matching (case-insensitive)
    #[serde(default)]
    pub target: Option<String>,

    /// Maximum number of symbols to return (default: 50).
    /// When set, truncates results to first N symbols
    /// Use 'target' parameter to filter to specific symbols instead of truncating
    /// Set to None for unlimited, or specific value to override default
    /// Example: limit=100 returns first 100 symbols
    #[serde(default = "default_limit")]
    pub limit: Option<u32>,

    /// Reading mode (default: "structure").
    /// - "structure": No bodies, structure only - quick overview
    /// - "minimal": Bodies for top-level symbols only - understand data structures
    /// - "full": Bodies for ALL symbols including nested methods - deep dive
    /// Recommended: "structure" for initial exploration, "minimal" for targeted body extraction
    #[serde(default = "default_mode")]
    pub mode: Option<String>,

    /// Workspace filter (optional): "primary" (default) or specific workspace ID
    /// Examples: "primary", "reference-workspace_abc123"
    /// Default: "primary" - search the primary workspace
    /// Note: Multi-workspace search ("all") is not supported - search one workspace at a time
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
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
        )
        .await
    }
}
