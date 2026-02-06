//! Shared git context capture for memory tools (checkpoint, plan_tool).

use std::process::Stdio;
use tokio::process::Command;
use tracing::warn;

use crate::handler::JulieServerHandler;
use super::GitContext;

/// Capture git context (branch, commit, dirty status, changed files) from the workspace.
///
/// Uses short commit hashes for consistency with recall display logic.
/// Returns `None` if not in a git repository or git commands fail.
pub async fn capture_git_context(handler: &JulieServerHandler) -> Option<GitContext> {
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
