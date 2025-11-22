//! Path utilities for julie-extractors

use anyhow::{Context, Result};
use std::path::{Path, MAIN_SEPARATOR};

/// Convert an absolute path to a relative Unix-style path
///
/// This is critical for cross-platform consistency - all file paths stored in the database
/// should be relative to the workspace root and use Unix-style separators.
pub fn to_relative_unix_style(absolute: &Path, workspace_root: &Path) -> Result<String> {
    // Try to canonicalize both paths to handle symlinks (e.g., /var -> /private/var on macOS)
    // If canonicalization fails (path doesn't exist), fall back to original paths
    let (path_to_use, root_to_use) = match (absolute.canonicalize(), workspace_root.canonicalize())
    {
        (Ok(canonical_abs), Ok(canonical_root)) => (canonical_abs, canonical_root),
        _ => (absolute.to_path_buf(), workspace_root.to_path_buf()),
    };

    // Windows UNC prefix handling: Strip \\?\ prefix for comparison
    #[cfg(windows)]
    fn strip_unc_prefix(path: &Path) -> std::path::PathBuf {
        let path_str = path.to_string_lossy();
        if path_str.starts_with(r"\\?\") {
            std::path::PathBuf::from(&path_str[4..])
        } else {
            path.to_path_buf()
        }
    }

    #[cfg(not(windows))]
    fn strip_unc_prefix(path: &Path) -> std::path::PathBuf {
        path.to_path_buf()
    }

    let normalized_path = strip_unc_prefix(&path_to_use);
    let normalized_root = strip_unc_prefix(&root_to_use);

    // Strip workspace prefix
    let relative = normalized_path
        .strip_prefix(&normalized_root)
        .with_context(|| {
            format!(
                "File path '{}' is not within workspace root '{}'",
                normalized_path.display(),
                normalized_root.display()
            )
        })?;

    // Convert to string and normalize separators to Unix-style
    let path_str = relative.to_str().context("Path contains invalid UTF-8")?;

    // Replace platform-specific separators with Unix-style /
    let unix_style = if MAIN_SEPARATOR == '\\' {
        path_str.replace('\\', "/")
    } else {
        path_str.to_string()
    };

    Ok(unix_style)
}
