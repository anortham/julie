//! Git identity of a checkout: which repository a directory belongs to.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The canonical common `.git` directory shared by every checkout of one repository.
///
/// A main checkout and each of its linked worktrees return the same path.
/// Returns `None` when `root` is not inside a git repository or git is unavailable.
pub fn git_common_dir(root: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let dir = PathBuf::from(text.trim());
    let dir = if dir.is_absolute() {
        dir
    } else {
        root.join(dir)
    };
    dir.canonicalize().ok()
}
