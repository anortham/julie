pub mod changed;
pub mod cli;
pub mod manifest;
pub mod runner;

pub use cli::TestCommand;
pub use manifest::{BucketConfig, TestManifest};
pub use runner::{
    BucketResult, BucketStatus, CommandExecutor, CommandOutcome, ProcessCommandExecutor,
    RunFailure, RunSummary,
};

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

pub fn workspace_root() -> PathBuf {
    resolve_workspace_root(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or_else(|error| panic!("failed to resolve xtask workspace root: {error}"))
}

fn resolve_workspace_root(manifest_dir: impl AsRef<Path>) -> Result<PathBuf> {
    let manifest_dir = manifest_dir.as_ref();
    let root = manifest_dir.parent().with_context(|| {
        format!(
            "expected xtask manifest dir to have a parent: {}",
            manifest_dir.display()
        )
    })?;

    if !root.join("Cargo.toml").is_file() {
        bail!(
            "expected workspace root to contain Cargo.toml: {}",
            root.display()
        );
    }

    Ok(root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::{resolve_workspace_root, workspace_root};

    #[test]
    fn manifest_tests_workspace_root_points_to_repository_root() {
        let root = workspace_root();

        assert!(root.join("Cargo.toml").is_file());
        assert!(root.join("xtask").is_dir());
    }

    #[test]
    fn manifest_tests_workspace_root_from_rejects_path_without_parent() {
        let error = resolve_workspace_root("/").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("expected xtask manifest dir to have a parent")
        );
    }
}
