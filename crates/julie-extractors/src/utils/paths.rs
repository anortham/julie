//! Path utilities for julie-extractors

use anyhow::{Context, Result};
use std::path::{MAIN_SEPARATOR, Path};

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
    let relative = match normalized_path.strip_prefix(&normalized_root) {
        Ok(relative) => relative,
        Err(error) => {
            if let Some(relative) =
                relative_by_normalized_string(&normalized_path, &normalized_root)
            {
                return Ok(relative);
            }

            return Err(error).with_context(|| {
                format!(
                    "File path '{}' is not within workspace root '{}'",
                    normalized_path.display(),
                    normalized_root.display()
                )
            });
        }
    };

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

fn relative_by_normalized_string(path: &Path, root: &Path) -> Option<String> {
    let path = path.to_string_lossy().replace('\\', "/");
    let root = root.to_string_lossy().replace('\\', "/");
    let root = root.trim_end_matches('/');

    if root.is_empty() {
        return None;
    }

    strip_normalized_prefix(&path, root).map(ToOwned::to_owned)
}

#[cfg(windows)]
fn strip_normalized_prefix<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    let path_lower = path.to_ascii_lowercase();
    let root_lower = root.to_ascii_lowercase();

    if path_lower == root_lower {
        return Some("");
    }

    let prefix = format!("{root_lower}/");
    path_lower
        .starts_with(&prefix)
        .then(|| &path[root.len() + 1..])
}

#[cfg(not(windows))]
fn strip_normalized_prefix<'a>(path: &'a str, root: &str) -> Option<&'a str> {
    if path == root {
        return Some("");
    }

    path.strip_prefix(&format!("{root}/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // Mirrors src/utils/paths.rs: the normalized-string fallback is duplicated
    // here, so it needs the same Windows coverage. Path::strip_prefix compares
    // components case-sensitively, so a case-mismatched component forces the
    // case-insensitive fallback (relative_by_normalized_string) to run.
    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_relative_fallback_handles_case_mismatch() {
        let workspace = PathBuf::from(r"C:\Users\Me\proj");
        let absolute = PathBuf::from(r"C:\Users\me\proj\src\main.rs");

        assert!(
            absolute.strip_prefix(&workspace).is_err(),
            "expected native strip_prefix to fail on case mismatch, forcing the fallback"
        );

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(result, "src/main.rs");
        assert!(!result.contains('\\'), "Should have no backslashes");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_relative_fallback_path_equals_root_modulo_case() {
        let workspace = PathBuf::from(r"C:\Users\Me\proj");
        let absolute = PathBuf::from(r"C:\Users\me\proj");

        assert!(
            absolute.strip_prefix(&workspace).is_err(),
            "expected native strip_prefix to fail on case mismatch, forcing the fallback"
        );

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(result, "");
    }
}
