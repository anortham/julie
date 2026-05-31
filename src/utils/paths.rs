// Julie's Path Conversion Utilities
//
// Handles conversion between absolute native paths and relative Unix-style paths
// for token-efficient storage and cross-platform compatibility.

use anyhow::{Context, Result};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

use crate::tools::navigation::resolution::{
    WorkspaceResolutionFailure, WorkspaceResolutionFailureKind,
};

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

#[derive(Debug)]
pub struct WorkspaceFileInputResolution {
    pub absolute_path: PathBuf,
    pub relative_query_path: String,
    pub canonicalized: bool,
}

/// Resolve a tool file input into the two path forms tool handlers need.
///
/// Tool inputs may be absolute, relative, contain `.` / `..`, or point at a
/// file that does not exist yet. This canonicalizes the input path when
/// possible, otherwise keeps the absolute candidate path, then computes a
/// relative Unix-style path for database queries.
///
/// # Strict contract — no raw-input fallback
///
/// If the resolved absolute path is **outside the workspace root**, this
/// function returns an `Err` wrapping [`WorkspaceResolutionFailure`] with
/// kind [`WorkspaceResolutionFailureKind::FileOutsideWorkspace`]. Callers
/// MUST propagate the error — they must not fall back to raw string
/// normalization of the input, which would let outside-workspace paths
/// silently reach the database as if they were workspace-relative.
///
/// At the MCP boundary, route this error through
/// `crate::handler::tools::error::classify_tool_failure`, which downcasts to
/// [`WorkspaceResolutionFailure`] and surfaces the result as
/// `McpError::invalid_params` so the user sees a clear "outside workspace"
/// message instead of an opaque internal error.
pub fn resolve_workspace_file_input(
    input: &str,
    workspace_root: &Path,
) -> Result<WorkspaceFileInputResolution> {
    let input_path = Path::new(input);
    let absolute_candidate = if input_path.is_absolute() {
        input_path.to_path_buf()
    } else {
        workspace_root.join(input_path)
    };

    let (absolute_path, canonicalized) = match absolute_candidate.canonicalize() {
        Ok(canonical) => (canonical, true),
        Err(_) => (absolute_candidate, false),
    };

    let relative_query_path =
        to_relative_unix_style(&absolute_path, workspace_root).map_err(|_| {
            WorkspaceResolutionFailure::new(
                WorkspaceResolutionFailureKind::FileOutsideWorkspace,
                format!("file path is outside the workspace: {}", input),
            )
        })?;

    Ok(WorkspaceFileInputResolution {
        absolute_path,
        relative_query_path,
        canonicalized,
    })
}

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
/// Strip the Windows `\\?\` extended-length (UNC) prefix for path comparison.
///
/// `std::fs::canonicalize()` returns paths with this prefix on Windows, but
/// non-canonical paths do not have it. Leaving it in place makes `strip_prefix`
/// fail even when one path is genuinely nested under the other. On non-Windows
/// targets this is a no-op clone.
fn strip_unc_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let path_str = path.to_string_lossy();
        if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
        path.to_path_buf()
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// Strip `workspace_root` from `path`, tolerating symlinked workspace roots
/// (e.g. macOS `/tmp` → `/private/tmp`, `/var` → `/private/var`) and deleted
/// leaf paths.
///
/// The file watcher receives event paths from `notify`, which on macOS reports
/// canonical (symlink-resolved) paths via FSEvents even when the workspace was
/// registered under a symlinked root. A naive `path.strip_prefix(workspace_root)`
/// then fails, and callers that fall back to inspecting the *absolute* path hit
/// false positives from ancestor directory names — e.g. `/private/tmp/proj/…`
/// contains the blacklisted component `tmp`, so delete/modify events for gone
/// files get silently dropped and leave orphaned symbols in the index.
///
/// Resolution order:
/// 1. Direct strip — both paths already share a symlink form (the common case:
///    canonical project root + canonical event paths, or raw root + raw paths).
/// 2. Canonicalize the root (which always exists, even when the leaf was
///    deleted) and retry against the candidate as-is. This recovers the relative
///    path whenever the candidate is already canonical, which is what `notify`
///    emits on macOS — and crucially does not require the leaf to exist.
/// 3. Canonicalize the candidate's current form (existing files only) and retry,
///    covering the reverse case where the candidate is raw but the root is
///    canonical (Windows junctions / symlinked candidates).
///
/// Returns the workspace-relative path, or `None` when `path` is genuinely not
/// inside `workspace_root`.
pub fn relative_within_workspace(path: &Path, workspace_root: &Path) -> Option<PathBuf> {
    if let Ok(rel) = path.strip_prefix(workspace_root) {
        return Some(rel.to_path_buf());
    }

    let canonical_root = match workspace_root.canonicalize() {
        Ok(root) => strip_unc_prefix(&root),
        Err(_) => return None,
    };

    let candidate = strip_unc_prefix(path);
    if let Ok(rel) = candidate.strip_prefix(&canonical_root) {
        return Some(rel.to_path_buf());
    }

    if let Ok(canonical_candidate) = path.canonicalize() {
        let canonical_candidate = strip_unc_prefix(&canonical_candidate);
        if let Ok(rel) = canonical_candidate.strip_prefix(&canonical_root) {
            return Some(rel.to_path_buf());
        }
    }

    None
}

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
    // On Unix, MAIN_SEPARATOR is already '/', so this is a no-op
    // On Windows, this converts '\' to '/'
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
