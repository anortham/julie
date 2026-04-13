// Julie's Utilities Module
//
// Common utilities and helper functions used throughout the Julie codebase.

use anyhow::Result;
use std::path::Path;

/// File utilities
pub mod file_utils {
    use super::*;
    use std::fs;

    /// Check if a file has a supported language extension.
    ///
    /// Delegates to `julie_extractors::language::detect_language_from_extension()`.
    pub fn is_supported_file(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(julie_extractors::language::detect_language_from_extension)
            .is_some()
    }

    /// Read file content safely
    pub fn read_file_content(path: &Path) -> Result<String> {
        Ok(fs::read_to_string(path)?)
    }

    /// Heuristic: does this file look like a text file by content?
    ///
    /// Reads the first 512 bytes. Rejects files with null bytes or fewer than
    /// 80% printable ASCII/UTF-8 bytes. Returns false for unreadable or empty
    /// files. Shared between workspace discovery (deciding what to index) and
    /// the startup freshness scan (deciding what is still present on disk), so
    /// both sides agree on whether an extensionless file like `Dockerfile` or
    /// `Makefile` belongs in the tracked set. See Finding #3 in
    /// `docs/ROOTS_IMPL_REVIEW_NOTES.md`.
    pub fn is_likely_text_file(file_path: &Path) -> bool {
        use std::io::Read;

        let mut file = match fs::File::open(file_path) {
            Ok(f) => f,
            Err(_) => return false,
        };
        let mut buffer = [0u8; 512];
        let bytes_read = match file.read(&mut buffer) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if bytes_read == 0 {
            return false;
        }
        if buffer[..bytes_read].contains(&0) {
            return false;
        }
        let printable = buffer[..bytes_read]
            .iter()
            .filter(|&&b| (32..=126).contains(&b) || b == 9 || b == 10 || b == 13 || b >= 128)
            .count();
        (printable as f64 / bytes_read as f64) > 0.8
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
                Component::RootDir => normalized.push(component.as_os_str()),
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

/// String similarity utilities for fuzzy matching
pub mod string_similarity;

/// Path conversion utilities (absolute ↔ relative Unix-style)
pub mod paths;

/// Shared file walker builder (wraps `ignore` crate for .gitignore + .julieignore support)
pub mod walk;

/// Lenient serde deserializers for MCP tool parameters (string-or-number u32)
pub mod serde_lenient;

/// Language detection utilities.
///
/// Delegates to `julie_extractors::language::detect_language_from_extension()`.
pub mod language {
    use std::path::Path;

    /// Detect programming language from file extension.
    pub fn detect_language(path: &Path) -> Option<&'static str> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(julie_extractors::language::detect_language_from_extension)
    }
}
