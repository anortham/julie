//! Seeds `blast_radius` from the working-tree git diff when no seed is given.

use std::path::Path;

use anyhow::{Context, Result};

pub fn changed_files(root: &Path) -> Result<Vec<String>> {
    let run = |args: &[&str]| -> Result<String> {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .with_context(|| format!("git {} in {}", args.join(" "), root.display()))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!(
                "git {} failed in {}: {stderr}",
                args.join(" "),
                root.display()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    };
    Ok(changed_files_from_git_output(
        &run(&["diff", "--name-only", "HEAD"])?,
        &run(&["ls-files", "--others", "--exclude-standard"])?,
    ))
}

pub fn changed_files_from_git_output(diff: &str, untracked: &str) -> Vec<String> {
    diff.lines()
        .chain(untracked.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}
