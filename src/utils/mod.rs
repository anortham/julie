// Julie's Utilities Module
//
// Common utilities and helper functions used throughout the Julie codebase.

use anyhow::Result;
use std::path::Path;

/// File utilities
pub mod file_utils {
    use super::*;
    use std::fs;

    /// Check if a file has a supported language extension
    pub fn is_supported_file(path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            matches!(
                ext,
                "rs" | "py"
                    | "js"
                    | "ts"
                    | "tsx"
                    | "jsx"
                    | "go"
                    | "java"
                    | "c"
                    | "cpp"
                    | "h"
                    | "hpp"
                    | "cs"
                    | "php"
                    | "rb"
                    | "swift"
                    | "kt"
                    | "lua"
                    | "gd"
                    | "vue"
                    | "html"
                    | "css"
                    | "sql"
                    | "sh"
                    | "bash"
                    | "r"
                    | "R"
                    | "md"        // Markdown
                    | "markdown"
                    | "json"      // JSON
                    | "jsonl"     // JSON Lines
                    | "jsonc"     // JSON with Comments (VSCode configs)
                    | "toml"      // TOML
                    | "yml"       // YAML
                    | "yaml"
            )
        } else {
            false
        }
    }

    /// Read file content safely
    pub fn read_file_content(path: &Path) -> Result<String> {
        Ok(fs::read_to_string(path)?)
    }

    /// Secure path resolution that prevents directory traversal attacks
    ///
    /// This function resolves a file path relative to a workspace root and ensures
    /// that the final resolved path is within the workspace boundaries to prevent
    /// path traversal security vulnerabilities.
    ///
    /// # Arguments
    /// * `file_path` - The file path to resolve (can be relative or absolute)
    /// * `workspace_root` - The workspace root directory
    ///
    /// # Returns
    /// * `Ok(PathBuf)` - The securely resolved absolute path within workspace
    /// * `Err` - If path traversal is detected
    ///
    /// # Security
    /// This function prevents attacks like:
    /// - `../../../etc/passwd` (relative traversal)
    /// - `/etc/passwd` (absolute path outside workspace)
    /// - Symlinks pointing outside workspace
    ///
    /// # Note
    /// Unlike canonicalize(), this works for non-existent files (needed for file creation).
    /// It manually resolves .. and . components to detect traversal attempts.
    pub fn secure_path_resolution(
        file_path: &str,
        workspace_root: &Path,
    ) -> Result<std::path::PathBuf> {
        use std::path::{Component, PathBuf};

        let candidate = Path::new(file_path);

        // Canonicalize workspace root (must exist)
        let canonical_workspace_root = workspace_root
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("Workspace root does not exist: {}", e))?;

        // Resolve to absolute path
        let resolved = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            canonical_workspace_root.join(candidate)
        };

        // Manually resolve path components to handle .. and . without requiring file existence
        let mut normalized = PathBuf::new();
        for component in resolved.components() {
            match component {
                Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
                Component::RootDir => normalized.push("/"),
                Component::CurDir => {} // Skip "."
                Component::ParentDir => {
                    // Pop parent, but track if we go above workspace root
                    if !normalized.pop() {
                        return Err(anyhow::anyhow!(
                            "Security: Path traversal attempt blocked. Path must be within workspace."
                        ));
                    }
                }
                Component::Normal(name) => normalized.push(name),
            }
        }

        // If file exists, canonicalize it to handle symlinks
        let final_path = if normalized.exists() {
            normalized
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("Failed to canonicalize existing path: {}", e))?
        } else {
            // For non-existent files, ensure parent directory is within workspace
            if let Some(parent) = normalized.parent() {
                if parent.exists() {
                    let canonical_parent = parent
                        .canonicalize()
                        .map_err(|e| anyhow::anyhow!("Parent directory does not exist: {}", e))?;
                    if !canonical_parent.starts_with(&canonical_workspace_root) {
                        return Err(anyhow::anyhow!(
                            "Security: Path traversal attempt blocked. Path must be within workspace."
                        ));
                    }
                }
            }
            normalized
        };

        // Final security check
        if !final_path.starts_with(&canonical_workspace_root) {
            return Err(anyhow::anyhow!(
                "Security: Path traversal attempt blocked. Path must be within workspace."
            ));
        }

        Ok(final_path)
    }
}

/// Token estimation utilities
pub mod token_estimation;

/// Context truncation utilities
pub mod context_truncation;

/// Progressive reduction utilities
pub mod progressive_reduction;

/// Cross-language intelligence utilities (THE secret sauce!)
pub mod cross_language_intelligence;

/// Path relevance scoring utilities
pub mod path_relevance;

/// Exact match boost utilities
pub mod exact_match_boost;

/// Query expansion utilities for multi-word search
pub mod query_expansion;

/// String similarity utilities for fuzzy matching
pub mod string_similarity;

/// Path conversion utilities (absolute ↔ relative Unix-style)
pub mod paths;

/// File ignore pattern utilities (.julieignore support)
pub mod ignore;

/// Language detection utilities
pub mod language {
    use std::path::Path;

    /// Detect programming language from file extension
    pub fn detect_language(path: &Path) -> Option<&'static str> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext {
                "rs" => Some("rust"),
                "py" => Some("python"),
                "js" => Some("javascript"),
                "ts" => Some("typescript"),
                "tsx" => Some("typescript"),
                "jsx" => Some("javascript"),
                "go" => Some("go"),
                "java" => Some("java"),
                "c" => Some("c"),
                "cpp" | "cc" | "cxx" => Some("cpp"),
                "h" => Some("c"),
                "hpp" | "hxx" => Some("cpp"),
                "cs" => Some("csharp"),
                "php" => Some("php"),
                "rb" => Some("ruby"),
                "swift" => Some("swift"),
                "kt" => Some("kotlin"),
                "lua" => Some("lua"),
                "gd" => Some("gdscript"),
                "vue" => Some("vue"),
                "html" => Some("html"),
                "css" => Some("css"),
                "sql" => Some("sql"),
                "sh" | "bash" => Some("bash"),
                "qml" => Some("qml"),
                "r" | "R" => Some("r"),
                _ => None,
            })
    }
}
