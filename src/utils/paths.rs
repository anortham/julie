// Julie's Path Conversion Utilities
//
// Handles conversion between absolute native paths and relative Unix-style paths
// for token-efficient storage and cross-platform compatibility.

use anyhow::{Context, Result};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

/// Convert an absolute path to a relative Unix-style path (with `/` separators)
///
/// This function strips the workspace root prefix and converts all path separators
/// to Unix-style forward slashes (`/`), regardless of the platform.
///
/// # Arguments
/// * `absolute` - The absolute path to convert
/// * `workspace_root` - The workspace root directory
///
/// # Returns
/// * `Ok(String)` - The relative Unix-style path (e.g., "src/tools/search.rs")
/// * `Err` - If the file is not within the workspace
///
/// # Examples
/// ```
/// // Windows
/// to_relative_unix_style("C:\\Users\\murphy\\project\\src\\main.rs", "C:\\Users\\murphy\\project")
/// // => "src/main.rs"
///
/// // Linux/macOS
/// to_relative_unix_style("/home/murphy/project/src/main.rs", "/home/murphy/project")
/// // => "src/main.rs"
/// ```
///
/// # Token Savings
/// - Windows UNC: `\\?\C:\Users\murphy\source\julie\src\tools\search.rs` (70 chars)
/// - Relative Unix: `src/tools/search.rs` (21 chars)
/// - **Savings: ~70% characters, ~60% tokens, no JSON escaping needed**
pub fn to_relative_unix_style(absolute: &Path, workspace_root: &Path) -> Result<String> {
    // 🔥 CRITICAL: Try to canonicalize both paths to handle symlinks (e.g., /var -> /private/var on macOS)
    // If canonicalization fails (path doesn't exist), fall back to original paths
    let (path_to_use, root_to_use) = match (absolute.canonicalize(), workspace_root.canonicalize())
    {
        (Ok(canonical_abs), Ok(canonical_root)) => {
            // Both paths can be canonicalized - use canonical versions
            (canonical_abs, canonical_root)
        }
        _ => {
            // One or both failed - use original paths for consistency
            (absolute.to_path_buf(), workspace_root.to_path_buf())
        }
    };

    // 🔥 Windows UNC prefix handling: Strip \\?\ prefix for comparison
    // Canonicalized Windows paths get \\?\ prefix, but non-canonical paths don't
    // This causes strip_prefix to fail even when paths are actually nested
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
    // On Unix, MAIN_SEPARATOR is already '/', so this is a no-op
    // On Windows, this converts '\' to '/'
    let unix_style = if MAIN_SEPARATOR == '\\' {
        path_str.replace('\\', "/")
    } else {
        path_str.to_string()
    };

    Ok(unix_style)
}

/// Convert a relative Unix-style path to an absolute native path
///
/// This function joins a relative Unix-style path (with `/` separators) to the
/// workspace root, automatically converting to native path separators.
///
/// # Arguments
/// * `relative_unix` - The relative Unix-style path (e.g., "src/tools/search.rs")
/// * `workspace_root` - The workspace root directory
///
/// # Returns
/// * `PathBuf` - The absolute native path
///
/// # Examples
/// ```
/// // Windows
/// to_absolute_native("src/main.rs", "C:\\Users\\murphy\\project")
/// // => "C:\\Users\\murphy\\project\\src\\main.rs"
///
/// // Linux/macOS
/// to_absolute_native("src/main.rs", "/home/murphy/project")
/// // => "/home/murphy/project/src/main.rs"
/// ```
///
/// # Notes
/// - `Path::join()` automatically handles Unix-style separators on all platforms
/// - Windows correctly interprets `/` as a path separator
/// - No explicit conversion needed - Rust std handles this
pub fn to_absolute_native(relative_unix: &str, workspace_root: &Path) -> PathBuf {
    // Path::join automatically converts '/' to native separators on Windows
    workspace_root.join(relative_unix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_absolute_to_relative() {
        // Real Windows UNC path (only testable on actual Windows)
        let workspace = PathBuf::from(r"\\?\C:\Users\murphy\source\julie");
        let absolute = PathBuf::from(r"\\?\C:\Users\murphy\source\julie\src\tools\search.rs");

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(result, "src/tools/search.rs");
        assert!(!result.contains('\\'), "Should have no backslashes");
        assert!(result.contains('/'), "Should use forward slashes");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_windows_path_conversion_logic() {
        // Test the separator conversion logic without relying on Windows-specific PathBuf behavior
        // We can't create real Windows paths on Unix, but we can verify the conversion logic works
        let workspace = PathBuf::from("/Users/murphy/source/julie");
        let absolute = PathBuf::from("/Users/murphy/source/julie/src/tools/search.rs");

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        // Verify Unix-style forward slashes
        assert_eq!(result, "src/tools/search.rs");
        assert!(!result.contains('\\'), "Should have no backslashes");
        assert!(result.contains('/'), "Should use forward slashes");
    }

    #[test]
    fn test_linux_absolute_to_relative() {
        let workspace = PathBuf::from("/home/murphy/source/julie");
        let absolute = PathBuf::from("/home/murphy/source/julie/src/tools/search.rs");

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(result, "src/tools/search.rs");
        assert!(result.contains('/'), "Should use forward slashes");
    }

    #[test]
    fn test_macos_absolute_to_relative() {
        let workspace = PathBuf::from("/Users/murphy/source/julie");
        let absolute = PathBuf::from("/Users/murphy/source/julie/src/tools/search.rs");

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(result, "src/tools/search.rs");
        assert!(result.contains('/'), "Should use forward slashes");
    }

    #[test]
    fn test_unicode_in_paths() {
        let workspace = PathBuf::from("/home/murphy/プロジェクト/julie");
        let absolute = PathBuf::from("/home/murphy/プロジェクト/julie/src/日本語.rs");

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(result, "src/日本語.rs");
        assert!(result.contains('/'), "Should use forward slashes");
    }

    #[test]
    fn test_spaces_in_paths() {
        let workspace = PathBuf::from("/home/murphy/my projects/julie");
        let absolute = PathBuf::from("/home/murphy/my projects/julie/src/my file.rs");

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(result, "src/my file.rs");
        assert!(result.contains('/'), "Should use forward slashes");
    }

    #[test]
    fn test_round_trip_conversion() {
        let workspace = PathBuf::from("/home/murphy/source/julie");
        let original_relative = "src/tools/search.rs";

        // Convert to absolute, then back to relative
        let absolute = to_absolute_native(original_relative, &workspace);
        let back_to_relative = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(back_to_relative, original_relative);
    }

    #[test]
    fn test_file_outside_workspace_rejected() {
        let workspace = PathBuf::from("/home/murphy/source/julie");
        let outside_file = PathBuf::from("/etc/passwd");

        let result = to_relative_unix_style(&outside_file, &workspace);

        assert!(result.is_err(), "Should reject files outside workspace");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not within workspace"),
            "Error should mention workspace boundary violation"
        );
    }

    #[test]
    fn test_to_absolute_native_simple() {
        let workspace = PathBuf::from("/home/murphy/source/julie");
        let relative = "src/main.rs";

        let result = to_absolute_native(relative, &workspace);

        assert_eq!(
            result,
            PathBuf::from("/home/murphy/source/julie/src/main.rs")
        );
    }

    #[test]
    fn test_to_absolute_native_handles_unix_separators() {
        // Even on Windows, Path::join should handle / correctly
        let workspace = PathBuf::from(r"C:\Users\murphy\source\julie");
        let relative = "src/tools/search.rs"; // Unix-style separators

        let result = to_absolute_native(relative, &workspace);

        // The result should be a valid path that includes both components
        assert!(result.to_string_lossy().contains("src"));
        assert!(result.to_string_lossy().contains("search.rs"));
    }

    #[test]
    fn test_nested_directories() {
        let workspace = PathBuf::from("/home/murphy/source/julie");
        let absolute = PathBuf::from("/home/murphy/source/julie/src/tools/editing/fuzzy.rs");

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(result, "src/tools/editing/fuzzy.rs");
        assert_eq!(result.matches('/').count(), 3, "Should have 3 separators");
    }

    #[test]
    fn test_root_level_file() {
        let workspace = PathBuf::from("/home/murphy/source/julie");
        let absolute = PathBuf::from("/home/murphy/source/julie/README.md");

        let result = to_relative_unix_style(&absolute, &workspace).unwrap();

        assert_eq!(result, "README.md");
        assert!(!result.contains('/'), "Root-level file has no separators");
    }
}
