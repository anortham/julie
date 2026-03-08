//! Git context capture for checkpoint memory.
//!
//! Captures current git state (branch, short commit hash, changed files,
//! untracked files) by shelling out to the `git` CLI. Used by checkpoint
//! to auto-attach git context.
//!
//! All failures are graceful — returns `None` if git is not installed,
//! the path is not a git repo, or any command fails.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

use super::GitContext;

/// Capture the current git context for a workspace root.
///
/// Returns `None` gracefully when:
/// - The path is not a git repository
/// - `git` is not installed or not in PATH
/// - Any git command fails for any reason
///
/// Changed files include staged changes, unstaged changes, and untracked
/// files (excluding those matched by `.gitignore`). Files are deduplicated
/// and returned in sorted order.
pub async fn get_git_context(workspace_root: &Path) -> Option<GitContext> {
    // Quick check: is this even a git repo?
    // `git rev-parse --is-inside-work-tree` exits non-zero if not.
    let check = run_git(workspace_root, &["rev-parse", "--is-inside-work-tree"]).await?;
    if check.trim() != "true" {
        return None;
    }

    // Branch name
    let branch = run_git(workspace_root, &["rev-parse", "--abbrev-ref", "HEAD"]).await
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Short commit hash
    let commit = run_git(workspace_root, &["rev-parse", "--short", "HEAD"]).await
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Changed files: staged + unstaged diffs against HEAD
    let diff_files = run_git(workspace_root, &["diff", "--name-only", "HEAD"]).await
        .unwrap_or_default();

    // Staged-only changes (files added to index but not yet committed)
    let staged_files = run_git(workspace_root, &["diff", "--name-only", "--cached"]).await
        .unwrap_or_default();

    // Untracked files (not in .gitignore)
    let untracked_files = run_git(
        workspace_root,
        &["ls-files", "--others", "--exclude-standard"],
    )
    .await
    .unwrap_or_default();

    // Merge and deduplicate using BTreeSet (sorted + unique)
    let mut all_files: BTreeSet<String> = BTreeSet::new();
    for output in [&diff_files, &staged_files, &untracked_files] {
        for line in output.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                all_files.insert(trimmed.to_string());
            }
        }
    }

    let files = if all_files.is_empty() {
        None
    } else {
        Some(all_files.into_iter().collect())
    };

    Some(GitContext {
        branch,
        commit,
        files,
    })
}

/// Run a git command in the given directory, returning stdout as a String.
///
/// Returns `None` if the command fails (non-zero exit) or cannot be spawned.
///
/// **Important**: Uses `kill_on_drop(true)` and pipes all stdio to prevent
/// hanging — a lesson learned from a previous bug (see checkpoint 030917_0b33).
async fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}
