//! Checkpoint Tool - Save immutable development memories
//!
//! Creates checkpoint memories that capture significant moments in development:
//! - Bug fixes and their solutions
//! - Feature implementations
//! - Architectural decisions
//! - Learning discoveries
//!
//! Each checkpoint is saved as a pretty-printed JSON file in `.memories/`
//! organized by date, making them git-trackable and human-readable.

use anyhow::{Context, Result};
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn};

use crate::handler::JulieServerHandler;
use crate::tools::memory::{GitContext, Memory, save_memory};

/// Capture git context from the workspace
async fn capture_git_context(handler: &JulieServerHandler) -> Option<GitContext> {
    // Get workspace root
    let workspace = handler.get_workspace().await.ok()??;
    let workspace_root = workspace.root.clone();

    // Get current branch
    let branch_output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(&workspace_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;

    if !branch_output.status.success() {
        warn!("Failed to get git branch - not a git repository?");
        return None;
    }

    let branch = String::from_utf8(branch_output.stdout)
        .ok()?
        .trim()
        .to_string();

    // Get current commit hash (short)
    let commit_output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&workspace_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;

    let commit = String::from_utf8(commit_output.stdout)
        .ok()?
        .trim()
        .to_string();

    // Check if working directory is dirty
    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&workspace_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;

    let dirty = !status_output.stdout.is_empty();

    // Get changed files (if dirty)
    let files_changed = if dirty {
        let diff_output = Command::new("git")
            .args(["diff", "--name-only", "HEAD"])
            .current_dir(&workspace_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .output()
            .await
            .ok()?;

        let files: Vec<String> = String::from_utf8(diff_output.stdout)
            .ok()?
            .lines()
            .map(|s| s.to_string())
            .collect();

        if files.is_empty() { None } else { Some(files) }
    } else {
        None
    };

    Some(GitContext {
        branch,
        commit,
        dirty,
        files_changed,
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CheckpointTool {
    /// Description of what was accomplished or learned
    pub description: String,
    /// Tags for categorization (e.g., ["bug", "auth", "performance"])
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// Memory type: "checkpoint" (default), "decision", "learning", "observation"
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<String>,
}

impl CheckpointTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("💾 Creating checkpoint: {}", self.description);

        // Get workspace root
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace available"))?;
        let workspace_root = workspace.root.clone();

        // Generate unique ID and timestamp
        let timestamp = chrono::Utc::now().timestamp();
        let memory_type = self
            .memory_type
            .clone()
            .unwrap_or_else(|| "checkpoint".to_string());
        let random_suffix = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();
        let id = format!("{}_{:x}_{}", memory_type, timestamp, random_suffix);

        // Capture git context
        let git_context = capture_git_context(handler).await;

        // Build extra fields (type-specific data)
        let mut extra_map = serde_json::Map::new();
        extra_map.insert("description".to_string(), json!(self.description.clone()));

        if let Some(ref tags) = self.tags {
            extra_map.insert("tags".to_string(), json!(tags.clone()));
        }

        let extra = serde_json::Value::Object(extra_map);

        // Create memory
        let memory = Memory {
            id: id.clone(),
            timestamp,
            memory_type,
            git: git_context.clone(),
            extra,
        };

        // Save to disk
        save_memory(&workspace_root, &memory).context("Failed to save memory checkpoint")?;

        // Keep output minimal - AI already knows what it saved
        let message = format!("✅ Checkpoint saved: {}", id);

        Ok(CallToolResult::text_content(vec![Content::text(
            message,
        )]))
    }
}
