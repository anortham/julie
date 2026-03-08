//! MCP tool wrapper for checkpoint save operations.
//!
//! Thin layer that converts MCP tool parameters into `CheckpointInput`
//! and delegates to `memory::checkpoint::save_checkpoint()`.

use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::memory::{CheckpointInput, CheckpointType};

#[derive(Debug, Deserialize, JsonSchema)]
/// Save a milestone checkpoint to developer memory with automatic git context capture.
pub struct CheckpointTool {
    /// Markdown description of the milestone. Write with structure (headers, bullets).
    /// Include WHAT was done, WHY it matters, HOW it works, and IMPACT on the codebase.
    /// This powers BM25 search — make it findable by future sessions.
    pub description: String,

    /// Checkpoint type: "checkpoint" (default), "decision", "incident", "learning".
    /// Use "decision" for architectural choices, "incident" for bugs/outages,
    /// "learning" for non-obvious discoveries. Affects how the checkpoint is presented.
    #[serde(default, rename = "type")]
    pub checkpoint_type: Option<String>,

    /// Tags for search and categorization. Think about discoverability — include
    /// synonyms and related terms (e.g. ["auth", "authentication", "jwt", "security"]).
    /// Powers BM25 full-text search across memories.
    #[serde(default)]
    pub tags: Option<Vec<String>>,

    /// Code symbols touched or affected (e.g. ["AuthMiddleware.handle", "refreshToken"]).
    /// Connects checkpoints to code for future reference lookups.
    #[serde(default)]
    pub symbols: Option<Vec<String>>,

    /// Decision statement (for type: "decision"). The actual decision made,
    /// stated clearly so future sessions know what to follow.
    #[serde(default)]
    pub decision: Option<String>,

    /// Alternatives that were considered and rejected (for type: "decision").
    /// Future sessions need to know what NOT to try again.
    #[serde(default)]
    pub alternatives: Option<Vec<String>>,

    /// Impact description — what changed as a result of this work.
    /// Include scope (files, modules, APIs affected) and severity.
    #[serde(default)]
    pub impact: Option<String>,

    /// Additional context that doesn't fit in description — background info,
    /// constraints, or environmental factors that influenced the work.
    #[serde(default)]
    pub context: Option<String>,

    /// Evidence or references — links, log snippets, error messages, benchmarks.
    /// Anything that supports the checkpoint's claims.
    #[serde(default)]
    pub evidence: Option<Vec<String>>,

    /// Open questions or unknowns that remain after this milestone.
    /// Future sessions should investigate these.
    #[serde(default)]
    pub unknowns: Option<Vec<String>>,

    /// Recommended next steps — what should happen after this milestone.
    /// Guides the next session's starting point.
    #[serde(default)]
    pub next: Option<String>,

    /// Confidence level (1-5). How confident are you in this work?
    /// 1 = exploratory/uncertain, 3 = solid but untested edge cases, 5 = bulletproof.
    #[serde(default)]
    pub confidence: Option<u8>,
}

impl CheckpointTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("Checkpoint save: {:?}", self.description);

        // Convert checkpoint_type string to CheckpointType enum
        let checkpoint_type = self.checkpoint_type.as_deref().and_then(parse_checkpoint_type);

        // Build CheckpointInput
        let input = CheckpointInput {
            description: self.description.clone(),
            checkpoint_type,
            tags: self.tags.clone(),
            symbols: self.symbols.clone(),
            decision: self.decision.clone(),
            alternatives: self.alternatives.clone(),
            impact: self.impact.clone(),
            context: self.context.clone(),
            evidence: self.evidence.clone(),
            unknowns: self.unknowns.clone(),
            next: self.next.clone(),
            confidence: self.confidence,
        };

        // Save the checkpoint
        let checkpoint =
            crate::memory::checkpoint::save_checkpoint(&handler.workspace_root, input).await?;

        // Format confirmation message
        let date = &checkpoint.timestamp[..10];
        let filename =
            crate::memory::storage::get_checkpoint_filename(&checkpoint.timestamp, &checkpoint.id);
        let rel_path = format!(".memories/{}/{}", date, filename);

        let summary = checkpoint
            .summary
            .as_deref()
            .unwrap_or(&checkpoint.description);

        let branch_info = checkpoint
            .git
            .as_ref()
            .map(|git| {
                let branch = git.branch.as_deref().unwrap_or("unknown");
                let commit = git.commit.as_deref().unwrap_or("unknown");
                format!("\n**Branch:** {} ({})", branch, commit)
            })
            .unwrap_or_default();

        let output = format!(
            "Checkpoint saved\n\
             **ID:** {}\n\
             **File:** {}\n\
             **Summary:** {}{}",
            checkpoint.id, rel_path, summary, branch_info
        );

        Ok(CallToolResult::text_content(vec![Content::text(output)]))
    }
}

/// Parse a checkpoint type string into the enum.
///
/// Returns `None` for unrecognized values (defaults to Checkpoint type).
fn parse_checkpoint_type(s: &str) -> Option<CheckpointType> {
    match s.to_lowercase().as_str() {
        "checkpoint" => Some(CheckpointType::Checkpoint),
        "decision" => Some(CheckpointType::Decision),
        "incident" => Some(CheckpointType::Incident),
        "learning" => Some(CheckpointType::Learning),
        _ => None,
    }
}
