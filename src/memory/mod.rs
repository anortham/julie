//! Memory system types and storage for developer checkpoint memory.
//!
//! This module implements the Goldfish-compatible memory system natively in Rust.
//! Checkpoints are stored as markdown files with YAML frontmatter in `.memories/`
//! directories, maintaining full backward compatibility with existing Goldfish files.
//!
//! ## File Layout
//!
//! ```text
//! {project}/.memories/
//! ├── 2026-03-07/
//! │   ├── 174414_7bb3.md    # HHMMSS_hash4.md
//! │   └── 213126_39de.md
//! └── plans/
//!     ├── my-plan.md
//!     └── .active-plan      # Contains active plan ID
//! ```

pub mod checkpoint;
pub mod git;
pub mod plan;
pub mod storage;

use serde::{Deserialize, Serialize};

// ============================================================================
// Core types — matching Goldfish TypeScript interfaces exactly
// ============================================================================

/// Type classification for a checkpoint.
///
/// Maps to Goldfish's `type?: 'checkpoint' | 'decision' | 'incident' | 'learning'`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckpointType {
    Checkpoint,
    Decision,
    Incident,
    Learning,
}

/// Git context captured at checkpoint time.
///
/// Maps to Goldfish's `GitContext` interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitContext {
    /// Current branch name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Short commit hash
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,

    /// Changed files (relative paths)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
}

/// A developer memory checkpoint.
///
/// Maps to Goldfish's `Checkpoint` interface. Field names use serde rename
/// attributes to match the Goldfish JSON/YAML format exactly (camelCase where
/// the TypeScript interface uses camelCase).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique identifier: `checkpoint_{SHA256(timestamp:description)[..8]}`
    pub id: String,

    /// ISO 8601 UTC timestamp
    pub timestamp: String,

    /// Markdown body content
    pub description: String,

    /// Classification type
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub checkpoint_type: Option<CheckpointType>,

    /// What problem/state triggered this work
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,

    /// The chosen approach (one sentence)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,

    /// Rejected alternatives and why
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternatives: Option<Vec<String>>,

    /// What changed, improved, or unblocked
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<String>,

    /// Verification evidence (tests, metrics, logs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Vec<String>>,

    /// Key symbols touched/affected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbols: Option<Vec<String>>,

    /// Follow-up action or open question
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,

    /// Confidence score (1-5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<u8>,

    /// Unresolved uncertainties or risks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unknowns: Option<Vec<String>>,

    /// Categorization tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    /// Git context at checkpoint time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitContext>,

    /// Auto-generated concise summary (for recall display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    /// ID of active plan when checkpoint was created
    #[serde(rename = "planId", skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
}

/// Input for creating a new checkpoint.
///
/// This is what callers pass to `checkpoint::save_checkpoint()`. All fields
/// except `description` are optional and default to `None`.
#[derive(Debug, Clone, Default)]
pub struct CheckpointInput {
    /// Markdown body content (required)
    pub description: String,

    /// Classification type (defaults to Checkpoint if None)
    pub checkpoint_type: Option<CheckpointType>,

    /// Categorization tags
    pub tags: Option<Vec<String>>,

    /// Key symbols touched/affected
    pub symbols: Option<Vec<String>>,

    /// The chosen approach (one sentence)
    pub decision: Option<String>,

    /// Rejected alternatives and why
    pub alternatives: Option<Vec<String>>,

    /// What changed, improved, or unblocked
    pub impact: Option<String>,

    /// What problem/state triggered this work
    pub context: Option<String>,

    /// Verification evidence (tests, metrics, logs)
    pub evidence: Option<Vec<String>>,

    /// Unresolved uncertainties or risks
    pub unknowns: Option<Vec<String>>,

    /// Follow-up action or open question
    pub next: Option<String>,

    /// Confidence score (1-5)
    pub confidence: Option<u8>,
}

/// A development plan with status tracking.
///
/// Maps to Goldfish's `Plan` interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// Plan identifier (slug from title)
    pub id: String,

    /// Human-readable title
    pub title: String,

    /// Markdown body content (without frontmatter)
    pub content: String,

    /// Status: "active", "completed", or "archived"
    pub status: String,

    /// ISO 8601 UTC creation timestamp
    pub created: String,

    /// ISO 8601 UTC last-updated timestamp
    pub updated: String,

    /// Categorization tags
    pub tags: Vec<String>,
}

/// Input for creating a new plan.
///
/// Used by `plan::save_plan()`. If `id` is `None`, it is auto-generated
/// by slugifying the `title` (e.g., "My Feature Plan" -> "my-feature-plan").
#[derive(Debug, Clone)]
pub struct PlanInput {
    /// Explicit plan ID (auto-generated from title if None)
    pub id: Option<String>,

    /// Human-readable title (required)
    pub title: String,

    /// Markdown body content (required)
    pub content: String,

    /// Categorization tags
    pub tags: Option<Vec<String>>,

    /// Whether to activate this plan after saving
    pub activate: Option<bool>,
}

/// Partial update for an existing plan.
///
/// Used by `plan::update_plan()`. Only `Some` fields are applied.
#[derive(Debug, Clone, Default)]
pub struct PlanUpdate {
    /// New title
    pub title: Option<String>,

    /// New markdown content
    pub content: Option<String>,

    /// New status ("active", "completed", "archived")
    pub status: Option<String>,

    /// New tags (replaces existing)
    pub tags: Option<Vec<String>>,
}

/// Options for recalling checkpoints.
///
/// Maps to Goldfish's `RecallOptions` interface.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecallOptions {
    /// Workspace filter: "current", "all", or a specific path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,

    /// Human-friendly ("2h", "30m", "3d") or ISO 8601 UTC
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,

    /// Look back N days
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,

    /// ISO 8601 UTC start of range
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,

    /// ISO 8601 UTC end of range
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,

    /// Fuzzy search query
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,

    /// Max checkpoints to return (default: 5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,

    /// Return full descriptions + all metadata (default: false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,

    /// Filter to checkpoints associated with this plan
    #[serde(rename = "planId", skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
}

/// Result from a recall operation.
///
/// Maps to Goldfish's `RecallResult` interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallResult {
    /// Retrieved checkpoints
    pub checkpoints: Vec<Checkpoint>,

    /// Currently active plan (if any)
    #[serde(rename = "activePlan", skip_serializing_if = "Option::is_none")]
    pub active_plan: Option<Plan>,

    /// Workspace summaries (when workspace="all")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspaces: Option<Vec<WorkspaceSummary>>,
}

/// Summary of checkpoint activity in a workspace (for cross-project recall).
///
/// Maps to Goldfish's `WorkspaceSummary` interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    /// Workspace name
    pub name: String,

    /// Absolute path to workspace root
    pub path: String,

    /// Number of checkpoints in this workspace
    #[serde(rename = "checkpointCount")]
    pub checkpoint_count: usize,

    /// ISO 8601 UTC timestamp of most recent checkpoint
    #[serde(rename = "lastActivity", skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<String>,
}
