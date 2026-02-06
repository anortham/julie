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
pub(crate) mod filtering;
mod formatting;
mod primary;
mod reference;

use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::CallToolResult;
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
    Some("minimal".to_string()) // Default to minimal for code output
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

//**********************//
//   Get Symbols Tool   //
//**********************//

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
    /// Reading mode: "structure", "minimal" (default), or "full"
    /// - "structure": Symbol names/signatures only (no code bodies)
    /// - "minimal": Code bodies for top-level symbols (default - enables code output)
    /// - "full": Code bodies for all symbols including nested
    #[serde(default = "default_mode")]
    pub mode: Option<String>,
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Output format: "json", "toon", "auto", or "code"
    /// Default: "code" when mode=minimal/full (code bodies available), otherwise "auto"
    /// - "code": Raw source code without metadata (optimal for AI reading)
    /// - "json": Full structured JSON with all metadata
    /// - "toon": Compact tabular format (35-70% token savings)
    /// - "auto": TOON for 5+ symbols, JSON otherwise
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
