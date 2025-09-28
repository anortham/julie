use crate::tools::shared::{BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS};
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tracing::debug;

impl ManageWorkspaceTool {
    pub(crate) fn discover_indexable_files(&self, workspace_path: &Path) -> Result<Vec<PathBuf>> {
        let mut indexable_files = Vec::new();
        let blacklisted_dirs: HashSet<&str> = BLACKLISTED_DIRECTORIES.iter().copied().collect();
        let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
        let max_file_size = 1024 * 1024; // 1MB limit for files

        debug!(
            "🔍 Starting recursive file discovery from: {}",
            workspace_path.display()
        );

        self.walk_directory_recursive(
            workspace_path,
            &blacklisted_dirs,
            &blacklisted_exts,
            max_file_size,
            &mut indexable_files,
        )?;

        debug!("📊 File discovery summary:");
        debug!("  - Total indexable files: {}", indexable_files.len());

        Ok(indexable_files)
    }

    /// Recursively walk directory tree, excluding blacklisted paths
    pub(crate) fn walk_directory_recursive(
        &self,
        dir_path: &Path,
        blacklisted_dirs: &HashSet<&str>,
        blacklisted_exts: &HashSet<&str>,
        max_file_size: u64,
        indexable_files: &mut Vec<PathBuf>,
    ) -> Result<()> {
        let entries = fs::read_dir(dir_path)
            .map_err(|e| anyhow::anyhow!("Failed to read directory {:?}: {}", dir_path, e))?;

        for entry in entries {
            let entry =
                entry.map_err(|e| anyhow::anyhow!("Failed to read directory entry: {}", e))?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip hidden files/directories that start with . (except known code files)
            if file_name.starts_with('.') && !self.is_known_dotfile(&path) {
                continue;
            }

            if path.is_dir() {
                // Check if directory should be blacklisted
                if blacklisted_dirs.contains(file_name) {
                    debug!("⏭️  Skipping blacklisted directory: {}", path.display());
                    continue;
                }

                // Recursively process subdirectory
                self.walk_directory_recursive(
                    &path,
                    blacklisted_dirs,
                    blacklisted_exts,
                    max_file_size,
                    indexable_files,
                )?;
            } else if path.is_file() {
                // Check file extension and size
                if self.should_index_file(&path, blacklisted_exts, max_file_size)? {
                    indexable_files.push(path);
                }
            }
        }

        Ok(())
    }

    /// Check if a file should be indexed based on blacklist and size limits
    pub(crate) fn should_index_file(
        &self,
        file_path: &Path,
        blacklisted_exts: &HashSet<&str>,
        max_file_size: u64,
    ) -> Result<bool> {
        // Get file extension
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!(".{}", ext.to_lowercase()))
            .unwrap_or_default();

        // Skip blacklisted extensions
        if blacklisted_exts.contains(extension.as_str()) {
            return Ok(false);
        }

        // Check file size
        let metadata = fs::metadata(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to get metadata for {:?}: {}", file_path, e))?;

        if metadata.len() > max_file_size {
            debug!(
                "⏭️  Skipping large file ({} bytes): {}",
                metadata.len(),
                file_path.display()
            );
            return Ok(false);
        }

        // If no extension, check if it's likely a text file by reading first few bytes
        if extension.is_empty() {
            return Ok(self.is_likely_text_file(file_path)?);
        }

        // Index any non-blacklisted file
        Ok(true)
    }

    /// Check if a dotfile is a known configuration file that should be indexed
    pub(crate) fn is_known_dotfile(&self, path: &Path) -> bool {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        matches!(
            file_name,
            ".gitignore"
                | ".gitattributes"
                | ".editorconfig"
                | ".eslintrc"
                | ".prettierrc"
                | ".babelrc"
                | ".tsconfig"
                | ".jsconfig"
                | ".cargo"
                | ".env"
                | ".npmrc"
        )
    }

    /// Heuristic to determine if a file without extension is likely a text file
    pub(crate) fn is_likely_text_file(&self, file_path: &Path) -> Result<bool> {
        // Read first 512 bytes to check for binary content
        let mut file = fs::File::open(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to open file {:?}: {}", file_path, e))?;

        let mut buffer = [0; 512];
        let bytes_read = file
            .read(&mut buffer)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", file_path, e))?;

        if bytes_read == 0 {
            return Ok(false); // Empty file
        }

        // Check for null bytes (common in binary files)
        let has_null_bytes = buffer[..bytes_read].contains(&0);
        if has_null_bytes {
            return Ok(false);
        }

        // Check if most bytes are printable ASCII/UTF-8
        let printable_count = buffer[..bytes_read]
            .iter()
            .filter(|&&b| b >= 32 && b <= 126 || b == 9 || b == 10 || b == 13 || b >= 128)
            .count();

        let text_ratio = printable_count as f64 / bytes_read as f64;
        Ok(text_ratio > 0.8) // At least 80% printable characters
    }
}
