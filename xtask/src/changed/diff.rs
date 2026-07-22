use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::mapping::{matches_exact, matches_prefix};

pub fn collect_changed_paths(workspace_root: &Path) -> Result<Vec<String>> {
    if let Some(paths) = std::env::var_os("XTASK_CHANGED_PATHS") {
        return Ok(normalize_paths(
            paths
                .to_string_lossy()
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        ));
    }

    let tracked_paths = if has_head(workspace_root)? {
        git_lines(
            workspace_root,
            &["diff", "--name-only", "--relative", "HEAD", "--"],
        )?
    } else {
        let mut paths = git_lines(workspace_root, &["diff", "--name-only", "--relative", "--"])?;
        paths.extend(git_lines(
            workspace_root,
            &["diff", "--name-only", "--relative", "--cached", "--"],
        )?);
        paths
    };
    let untracked_paths = git_lines(
        workspace_root,
        &["ls-files", "--others", "--exclude-standard"],
    )?;

    Ok(normalize_paths(
        tracked_paths
            .into_iter()
            .chain(untracked_paths)
            .collect::<Vec<_>>(),
    ))
}

fn has_head(workspace_root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .with_context(|| format!("failed to check git HEAD in {}", workspace_root.display()))?;
    Ok(output.status.success())
}

fn git_lines(workspace_root: &Path, args: &[&str]) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace_root)
        .output()
        .with_context(|| {
            format!(
                "failed to run git {:?} in {}",
                args,
                workspace_root.display()
            )
        })?;

    if !output.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub(super) fn normalize_paths(paths: Vec<String>) -> Vec<String> {
    paths
        .into_iter()
        .map(|path| normalize_path(&path))
        .filter(|path| !path.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_path(path: &str) -> String {
    path.trim()
        .trim_start_matches("./")
        .replace('\\', "/")
        .trim()
        .to_string()
}

pub(super) fn should_ignore(path: &str) -> bool {
    if matches_exact(
        path,
        &[
            "docs/TESTING_GUIDE.md",
            "docs/plans/verification-ledger-template.md",
        ],
    ) {
        return false;
    }

    matches_prefix(path, &[".julie/", ".memories/", "docs/"])
        || matches_exact(path, &[".DS_Store"])
        || path.starts_with("target/")
}
