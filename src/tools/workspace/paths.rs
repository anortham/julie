use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::info;

impl ManageWorkspaceTool {
    pub(crate) fn resolve_workspace_path(&self, workspace_path: Option<String>) -> Result<PathBuf> {
        let target_path = match workspace_path {
            Some(path) => {
                let expanded_path = shellexpand::tilde(&path).to_string();
                PathBuf::from(expanded_path)
            }
            None => std::env::current_dir()?,
        };

        // Ensure path exists
        if !target_path.exists() {
            return Err(anyhow::anyhow!(
                "Path does not exist: {}",
                target_path.display()
            ));
        }

        // If it's a file, get its directory
        let workspace_candidate = if target_path.is_file() {
            target_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Cannot determine parent directory"))?
                .to_path_buf()
        } else {
            target_path
        };

        // Find the actual workspace root
        self.find_workspace_root(&workspace_candidate)
    }

    /// Find workspace root by looking for common workspace markers
    pub(crate) fn find_workspace_root(&self, start_path: &Path) -> Result<PathBuf> {
        let workspace_markers = [
            ".git",
            ".julie",
            ".vscode",
            "Cargo.toml",
            "package.json",
            ".project",
        ];

        let mut current_path = start_path.to_path_buf();

        // Walk up the directory tree looking for workspace markers
        loop {
            for marker in &workspace_markers {
                let marker_path = current_path.join(marker);
                if marker_path.exists() {
                    info!(
                        "🎯 Found workspace marker '{}' at: {}",
                        marker,
                        current_path.display()
                    );
                    return Ok(current_path);
                }
            }

            match current_path.parent() {
                Some(parent) => current_path = parent.to_path_buf(),
                None => break,
            }
        }

        // No markers found, use the original path as workspace root
        info!(
            "🎯 No workspace markers found, using directory as root: {}",
            start_path.display()
        );
        Ok(start_path.to_path_buf())
    }
}
