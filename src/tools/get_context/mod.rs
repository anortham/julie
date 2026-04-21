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
pub mod second_hop;
pub mod task_signals;

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

    /// Token budget override (default: auto-scaled 2000-4000 based on result count). Set higher for broad exploration, lower for focused queries
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_u32_lenient"
    )]
    pub max_tokens: Option<u32>,

    /// Workspace filter: "primary" (default) or a workspace ID
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

    /// File paths edited in the current task. Boosts pivots and neighbors in those files.
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_vec_string_lenient"
    )]
    pub edited_files: Option<Vec<String>>,

    /// Explicit symbol entry points for the current task.
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_vec_string_lenient"
    )]
    pub entry_symbols: Option<Vec<String>>,

    /// Optional stack trace or file:line list for the current task.
    #[serde(default)]
    pub stack_trace: Option<String>,

    /// Named failing test file or symbol for the current task.
    #[serde(default)]
    pub failing_test: Option<String>,

    /// Maximum graph hop depth. Defaults to 1, with 2 enabling bounded second-hop expansion.
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_u32_lenient"
    )]
    pub max_hops: Option<u32>,

    /// Let test-linked symbols compete for neighbor slots when true.
    #[serde(default)]
    pub prefer_tests: Option<bool>,
}

impl GetContextTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let result = pipeline::run(self, handler).await?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }
}
