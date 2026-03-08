//! MCP tool wrapper for recall operations.
//!
//! Thin layer that converts MCP tool parameters into `RecallOptions`
//! and delegates to `memory::recall::recall()`.

use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::memory::RecallOptions;

#[derive(Debug, Deserialize, JsonSchema)]
/// Retrieve prior context from developer memory. Returns recent checkpoints
/// and the active plan.
pub struct RecallTool {
    /// Max checkpoints to return (default: 5, 0 = active plan only)
    #[serde(default)]
    pub limit: Option<u32>,

    /// Time filter: "2h", "30m", "3d", "1w", or ISO timestamp
    #[serde(default)]
    pub since: Option<String>,

    /// Look back N days
    #[serde(default)]
    pub days: Option<u32>,

    /// Date range start (YYYY-MM-DD or ISO timestamp)
    #[serde(default)]
    pub from: Option<String>,

    /// Date range end (YYYY-MM-DD or ISO timestamp)
    #[serde(default)]
    pub to: Option<String>,

    /// Search query (BM25 full-text search over memories)
    #[serde(default)]
    pub search: Option<String>,

    /// Return full descriptions + git metadata (default: false)
    #[serde(default)]
    pub full: Option<bool>,

    /// Workspace scope: "current" (default) or "all" (cross-project, daemon mode only)
    #[serde(default)]
    pub workspace: Option<String>,

    /// Filter to checkpoints under a specific plan
    #[serde(default, rename = "planId")]
    pub plan_id: Option<String>,
}

impl RecallTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("Recall: limit={:?}, search={:?}", self.limit, self.search);

        // Build RecallOptions from tool parameters
        let options = RecallOptions {
            workspace: self.workspace.clone(),
            since: self.since.clone(),
            days: self.days,
            from: self.from.clone(),
            to: self.to.clone(),
            search: self.search.clone(),
            limit: self.limit.map(|l| l as usize),
            full: self.full,
            plan_id: self.plan_id.clone(),
        };

        // Call recall
        let result = crate::memory::recall::recall(&handler.workspace_root, options)?;

        // Format output
        let output = format_recall_result(&result);

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }
}

/// Format a `RecallResult` as readable markdown text.
pub(crate) fn format_recall_result(result: &crate::memory::RecallResult) -> String {
    let mut output = String::new();

    // Checkpoints section
    if result.checkpoints.is_empty() {
        output.push_str("## Checkpoints (0 found)\n\nNo checkpoints found.\n");
    } else {
        output.push_str(&format!(
            "## Checkpoints ({} found)\n",
            result.checkpoints.len()
        ));

        for cp in &result.checkpoints {
            output.push_str(&format!("\n### {} -- {}\n", cp.id, cp.timestamp));

            if let Some(ref summary) = cp.summary {
                output.push_str(&format!("**Summary:** {}\n", summary));
            }

            if let Some(ref tags) = cp.tags {
                if !tags.is_empty() {
                    output.push_str(&format!("**Tags:** {}\n", tags.join(", ")));
                }
            }

            if let Some(ref git) = cp.git {
                let branch = git.branch.as_deref().unwrap_or("unknown");
                let commit = git.commit.as_deref().unwrap_or("unknown");
                output.push_str(&format!("**Branch:** {} ({})\n", branch, commit));
            }

            if let Some(ref plan_id) = cp.plan_id {
                output.push_str(&format!("**Plan:** {}\n", plan_id));
            }

            output.push('\n');
            output.push_str(&cp.description);
            output.push_str("\n\n---\n");
        }
    }

    // Active plan section
    if let Some(ref plan) = result.active_plan {
        output.push_str(&format!("\n## Active Plan: {}\n", plan.title));
        output.push_str(&format!("**ID:** {} | **Status:** {}\n", plan.id, plan.status));
        if !plan.tags.is_empty() {
            output.push_str(&format!("**Tags:** {}\n", plan.tags.join(", ")));
        }
        output.push('\n');
        output.push_str(&plan.content);
        output.push('\n');
    }

    output
}
