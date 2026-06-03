//! Path conversion utilities shared across the Julie workspace.
//!
//! Lives in `julie-core` (the bottom leaf crate) so that `julie-core::database`
//! and other sibling crates can call these helpers without depending on the full
//! `julie` crate. `julie::utils::paths` re-exports the public surface so all
//! existing `crate::utils::paths::*` call sites compile unchanged.

use anyhow::{Context, Result};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};

// ──────────────────────────────────────────────────────────────────────────────
// strip_unc_prefix
// ──────────────────────────────────────────────────────────────────────────────

/// Strip the Windows `\\?\` extended-length (UNC) prefix for path comparison.
///
/// `std::fs::canonicalize()` returns paths with this prefix on Windows, but
/// non-canonical paths do not have it. Leaving it in place makes `strip_prefix`
/// fail even when one path is genuinely nested under the other. On non-Windows
/// targets this is a no-op clone.
///
/// Exposed as `pub` so `julie::utils::paths::relative_within_workspace` (which
/// stays in the main crate) can re-import it without duplication.
pub fn strip_unc_prefix(path: &Path) -> PathBuf {
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

// ──────────────────────────────────────────────────────────────────────────────
// to_relative_unix_style (+ private helpers)
// ──────────────────────────────────────────────────────────────────────────────

/// Convert an absolute path to a relative Unix-style path (with `/` separators).
///
/// Strips the workspace root prefix and normalises all path separators to
/// forward slashes, regardless of platform.
///
/// # Arguments
/// * `absolute`       — The absolute path to convert.
/// * `workspace_root` — The workspace root directory.
///
/// # Returns
/// * `Ok(String)` — Relative Unix-style path (e.g. `"src/tools/search.rs"`).
/// * `Err`        — If the file is not within the workspace.
///
/// # Token savings
/// - Windows UNC: `\\?\C:\Users\murphy\source\julie\src\tools\search.rs` (70 chars)
/// - Relative Unix: `src/tools/search.rs` (21 chars) — ~70% characters, ~60% tokens
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
