//! Get Context tool — token-budgeted code context subgraph
//!
//! Given a query, returns a curated set of code symbols with full bodies for pivots
//! and signatures for neighbors, all within a token budget. Use at the start of a
//! task for orientation.

pub mod allocation;
pub mod content;
pub mod formatting;
pub mod pipeline;
pub mod scoring;

use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, Content};

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

#[derive(Debug, Deserialize, JsonSchema)]
/// Get token-budgeted context for a concept or task. Returns relevant code subgraph
/// with pivots (full code) and neighbors (signatures). Use at the start of a task
/// for orientation.
pub struct GetContextTool {
    /// Search query (text or pattern)
    pub query: String,

    /// Maximum token budget for the response (default: auto-scaled based on result count)
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,

    /// Language filter: "rust", "typescript", "python", etc.
    #[serde(default)]
    pub language: Option<String>,

    /// File pattern filter (glob syntax)
    #[serde(default)]
    pub file_pattern: Option<String>,

    /// Output format: "compact" (default) or "readable"
    #[serde(default)]
    pub format: Option<String>,
}

impl GetContextTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let result = pipeline::run(self, handler).await?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }
}
