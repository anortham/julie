//! Pure-filesystem workspace root markers for use in both top-crate and
//! julie-runtime test binaries.
//!
//! These two helpers create the `.git` sentinel that `find_workspace_root`
//! uses to stop its upward walk. They have zero dependency on handler, daemon,
//! or pool types and may be used anywhere.

use std::path::{Path, PathBuf};

/// Create an isolated workspace root directory under `parent/name` and mark it
/// with a `.git` directory so `find_workspace_root` stops there.
pub fn make_isolated_workspace_root(parent: &Path, name: &str) -> PathBuf {
    let root = parent.join(name);
    std::fs::create_dir_all(&root).expect("create temp workspace root");
    mark_workspace_root(root.as_path());
    root
}

/// Drop a `.git` marker inside `dir` so `find_workspace_root` treats it as a
/// workspace boundary. Idempotent.
pub fn mark_workspace_root(dir: &Path) {
    std::fs::create_dir_all(dir.join(".git")).expect("create workspace root marker");
}
