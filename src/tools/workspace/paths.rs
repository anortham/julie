/// Get the global Julie home directory (~/.julie/).
fn julie_home() -> anyhow::Result<std::path::PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(std::path::PathBuf::from(home).join(".julie"))
}
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::env;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

impl ManageWorkspaceTool {
    /// Resolve the workspace path from an explicit path, handler root, env var, or cwd.
    ///
    /// Priority: explicit path > handler_root > JULIE_WORKSPACE env > current_dir()
    pub(crate) fn resolve_workspace_path(
        &self,
        workspace_path: Option<String>,
        handler_root: Option<&Path>,
    ) -> Result<PathBuf> {
        let target_path = match workspace_path {
            Some(path) => {
                let expanded_path = shellexpand::tilde(&path).to_string();
                PathBuf::from(expanded_path)
            }
            None => {
                // Priority: handler_root > JULIE_WORKSPACE env > current_dir
                if let Some(root) = handler_root {
                    root.to_path_buf()
                } else if let Ok(path_str) = env::var("JULIE_WORKSPACE") {
                    let expanded = shellexpand::tilde(&path_str).to_string();
                    let path = PathBuf::from(expanded);
                    if path.exists() {
                        path.canonicalize().unwrap_or(path)
                    } else {
                        env::current_dir()?
                    }
                } else {
                    env::current_dir()?
                }
            }
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

    /// Returns true if the given `.julie` directory path matches the global config dir (~/.julie/).
    fn is_global_julie_dir(julie_dir_path: &Path, global_julie_home: &Option<PathBuf>) -> bool {
        global_julie_home.as_ref().map_or(false, |home| {
            julie_dir_path
                .canonicalize()
                .unwrap_or_else(|_| julie_dir_path.to_path_buf())
                == home.canonicalize().unwrap_or_else(|_| home.clone())
        })
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

        let global_julie_home = julie_home().ok();

        // 🔥 CRITICAL FIX: Check if start_path itself has a .julie directory FIRST
        // This prevents walking up and finding a parent workspace when an explicit
        // workspace path is provided (fixes fixture test isolation bug)
        let julie_dir = start_path.join(".julie");
        if julie_dir.exists() && julie_dir.is_dir() {
            if Self::is_global_julie_dir(&julie_dir, &global_julie_home) {
                debug!(
                    "Skipping global ~/.julie/ config dir at: {}",
                    start_path.display()
                );
            } else {
                debug!(
                    "Found .julie directory at provided path: {}",
                    start_path.display()
                );
                info!(
                    "🎯 Found .julie directory at provided path: {}",
                    start_path.display()
                );
                return Ok(start_path.to_path_buf());
            }
        }
        debug!("No .julie at start_path, walking up the tree");

        let mut current_path = start_path.to_path_buf();

        // Walk up the directory tree looking for workspace markers
        loop {
            for marker in &workspace_markers {
                let marker_path = current_path.join(marker);
                if marker_path.exists() {
                    // Skip ~/.julie/ global config dir — it's not a workspace marker
                    if *marker == ".julie"
                        && Self::is_global_julie_dir(&marker_path, &global_julie_home)
                    {
                        debug!(
                            "Skipping global ~/.julie/ config dir during walk-up at: {}",
                            current_path.display()
                        );
                        continue;
                    }
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
