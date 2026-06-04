// Julie's Path Conversion Utilities
//
// Handles conversion between absolute native paths and relative Unix-style paths
// for token-efficient storage and cross-platform compatibility.

use std::path::{Path, PathBuf};

// to_relative_unix_style, WorkspaceFileInputResolution, resolve_workspace_file_input,
// and relative_within_workspace live in julie-core; re-exported publicly so all
// crate::utils::paths::* callers compile unchanged.
pub use julie_core::paths::{WorkspaceFileInputResolution, relative_within_workspace, resolve_workspace_file_input, to_relative_unix_style};

/// Convert a path to a user-friendly display string.
///
/// On Windows, `std::fs::canonicalize()` returns paths with the `\\?\` UNC
/// extended-length prefix (e.g. `\\?\C:\Users\alice\project`). This prefix is
/// functionally correct but confusing in API responses and UI. This helper
/// strips it for user-facing output.
pub fn display_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    // UNC extended path: \\?\UNC\server\share → \\server\share
    if let Some(unc_rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{}", unc_rest)
    // Local extended path: \\?\C:\Users\... → C:\Users\...
    } else if let Some(stripped) = s.strip_prefix(r"\\?\") {
        stripped.to_string()
    } else {
        s.into_owned()
    }
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

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_relative_fallback_handles_case_mismatch() {
        // Non-existent paths, so canonicalize() is skipped and Path::strip_prefix
        // runs on the raw paths. strip_prefix compares components case-sensitively,
        // so "Me" != "me" makes it fail — exercising the case-insensitive,
        // separator-normalized fallback (relative_by_normalized_string) that the
        // native strip_prefix cannot satisfy on Windows.
        let workspace = PathBuf::from(r"C:\Users\Me\proj");
        let absolute = PathBuf::from(r"C:\Users\me\proj\src\main.rs");

        // Precondition: native strip_prefix compares Normal components
        // case-sensitively, so "Me" vs "me" makes it fail. Asserting this
        // guarantees the Ok result below is produced by the fallback, not the
        // happy path (and fails loudly if that ever stops being true).
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
        // path == root differing only by case: strip_prefix fails, and the
        // fallback's equality branch returns "" — this also guards the
        // &path[root.len() + 1..] slice from ever being indexed out of range.
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
