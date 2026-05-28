//! File filtering logic for watcher operations
//!
//! This module provides utilities for determining which files should be indexed
//! based on extension and ignore patterns.

use crate::tools::shared::BLACKLISTED_DIRECTORIES;
use crate::tools::workspace::indexing::file_policy;
use anyhow::Result;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::collections::HashSet;
use std::path::Path;
use tracing::warn;

/// Build set of supported file extensions.
///
/// Derived from the canonical `julie_extractors::language::supported_extensions()`.
pub fn build_supported_extensions() -> HashSet<String> {
    julie_extractors::language::supported_extensions()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Walk `dir` recursively (up to `max_depth` levels) and collect paths of all
/// `.gitignore` files found, skipping blacklisted and hidden directories.
///
/// Hidden directories (starting with `.`) are skipped because they are almost
/// universally tool caches (`.ruff_cache`, `.pytest_cache`, `.mypy_cache`,
/// `.venv`, etc.) whose `.gitignore` files contain bare `*` patterns. The
/// `ignore` crate's `matched_path_or_any_parents` does not properly scope
/// these patterns to their parent directory, causing them to match ALL files
/// globally and silently breaking the file watcher.
fn collect_gitignore_files(dir: &Path, max_depth: usize) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    if max_depth == 0 {
        return found;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return found,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip .git, blacklisted dirs, and hidden dirs (tool caches with
            // overly broad * patterns that leak via the ignore crate).
            if name == ".git" || name.starts_with('.') || BLACKLISTED_DIRECTORIES.contains(&name) {
                continue;
            }
            found.extend(collect_gitignore_files(&path, max_depth - 1));
        } else if path.file_name().and_then(|n| n.to_str()) == Some(".gitignore") {
            found.push(path);
        }
    }
    found
}

/// Build a gitignore-based matcher that layers:
/// 1. `.gitignore` patterns from the workspace root and all subdirectories
/// 2. `.julieignore` patterns (if present in workspace root)
/// 3. Synthetic patterns for Julie's own directories and common noise
pub fn build_gitignore_matcher(workspace_root: &Path) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(workspace_root);

    // Add root .gitignore first so its rules take precedence
    let gitignore_path = workspace_root.join(".gitignore");
    if gitignore_path.is_file() {
        if let Some(err) = builder.add(&gitignore_path) {
            warn!(
                "Partial error reading {}: {}",
                gitignore_path.display(),
                err
            );
        }
    }

    // Add .gitignore files from subdirectories (up to 8 levels deep).
    // Each subdirectory gitignore anchors its patterns to its own directory.
    for sub_gitignore in collect_gitignore_files(workspace_root, 8) {
        if sub_gitignore == gitignore_path {
            continue; // already added root
        }
        if let Some(err) = builder.add(&sub_gitignore) {
            warn!("Partial error reading {}: {}", sub_gitignore.display(), err);
        }
    }

    let julieignore_path = workspace_root.join(".julieignore");
    if julieignore_path.is_file() {
        if let Some(err) = builder.add(&julieignore_path) {
            warn!(
                "Partial error reading {}: {}",
                julieignore_path.display(),
                err
            );
        }
    }

    let synthetics = [
        ".julie/",
        ".memories/",
        "cmake-build-*/",
        "*.min.js",
        "*.bundle.js",
    ];
    for pattern in &synthetics {
        builder
            .add_line(None, pattern)
            .map_err(|e| anyhow::anyhow!("Invalid synthetic pattern '{}': {}", pattern, e))?;
    }

    builder
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build gitignore matcher: {}", e))
}

/// Check if any component of the path is a blacklisted directory name.
///
/// This catches directories like `node_modules`, `.git`, `target`, `bin`, `obj`, etc.
/// regardless of where they appear in the path hierarchy.
///
/// When `workspace_root` is provided, the check is performed on the relative path
/// (stripping the root prefix). This avoids false positives from system directories
/// in the absolute path (e.g., `AppData` on Windows temp paths).
/// Falls back to checking the full path if the prefix cannot be stripped.
pub fn contains_blacklisted_directory(path: &Path) -> bool {
    contains_blacklisted_directory_relative(path, None)
}

/// Like [`contains_blacklisted_directory`] but with an explicit workspace root
/// to relativize the path before checking.
pub fn contains_blacklisted_directory_relative(path: &Path, workspace_root: Option<&Path>) -> bool {
    // Relativize against the workspace root so only directory names *inside* the
    // workspace are blacklist-checked. The symlink-tolerant helper recovers the
    // relative path even when the event path is canonical and the root is
    // symlinked (macOS `/tmp` → `/private/tmp`); without it, the absolute path
    // would be checked and ancestor names like `tmp` would falsely match,
    // silently dropping legitimate delete/modify events. When no root is given
    // (or relativization genuinely fails), fall back to the full path.
    let relative =
        workspace_root.and_then(|root| crate::utils::paths::relative_within_workspace(path, root));
    let check_path = relative.as_deref().unwrap_or(path);
    check_path.components().any(|c| {
        if let std::path::Component::Normal(name) = c {
            if let Some(s) = name.to_str() {
                return BLACKLISTED_DIRECTORIES.contains(&s);
            }
        }
        false
    })
}

/// Check if a path is ignored by the gitignore matcher.
///
/// Strips the workspace root prefix and checks the relative path against
/// the gitignore rules (including parent directory matching).
pub fn is_gitignored(path: &Path, gitignore: &Gitignore, workspace_root: &Path) -> bool {
    // Symlink-tolerant relativization so gitignore patterns anchor correctly
    // even when the event path is canonical but the workspace root is symlinked.
    let rel_path = match crate::utils::paths::relative_within_workspace(path, workspace_root) {
        Some(p) => p,
        None => return false,
    };
    gitignore
        .matched_path_or_any_parents(&rel_path, path.is_dir())
        .is_ignore()
}

/// Returns true if `path` is under the configured JULIE_HOME and must be
/// excluded from indexing. Resolving `DaemonPaths::try_new()` may fail
/// (empty env, no home dir); we treat that as "no exclusion" since the
/// daemon cannot be running anyway, and the conventional `~/.julie`
/// fallback is handled separately by the workspace-root finder.
fn is_under_configured_julie_home(path: &Path) -> bool {
    match crate::paths::DaemonPaths::try_new() {
        Ok(paths) => paths.is_under_julie_home(path),
        Err(_) => false,
    }
}

/// Check if a file should be indexed based on extension, blacklists, and gitignore.
///
/// Layers (in order):
/// 1. Must be an existing file on disk
/// 2. Must NOT live under the configured JULIE_HOME (defends against
///    operators pointing `JULIE_HOME` inside a workspace tree, which
///    would otherwise leak daemon tokens, transport metadata, discovery
///    state, etc. into the index)
/// 3. Filename must not be blacklisted (lockfiles, etc.)
/// 4. Extension must be in supported set
/// 5. No path component may be a blacklisted directory
/// 6. Must not match gitignore/julieignore/synthetic patterns
pub fn should_index_file(
    path: &Path,
    supported_extensions: &HashSet<String>,
    gitignore: &Gitignore,
    workspace_root: &Path,
) -> bool {
    if !path.is_file() {
        return false;
    }
    if is_under_configured_julie_home(path) {
        return false;
    }
    if !file_policy::should_watch_path(path, supported_extensions) {
        return false;
    }
    if contains_blacklisted_directory_relative(path, Some(workspace_root)) {
        return false;
    }
    if is_gitignored(path, gitignore, workspace_root) {
        return false;
    }
    true
}

/// Check if a deletion event should be processed.
///
/// Same filtering as `should_index_file` but:
/// - Skips the `is_file()` check (the file is already gone)
/// - Hardcodes `is_dir: false` for gitignore matching (deleted paths are treated as files)
pub fn should_process_deletion(
    path: &Path,
    supported_extensions: &HashSet<String>,
    gitignore: &Gitignore,
    workspace_root: &Path,
) -> bool {
    if is_under_configured_julie_home(path) {
        return false;
    }
    if !file_policy::should_process_deleted_path(path, supported_extensions) {
        return false;
    }
    if contains_blacklisted_directory_relative(path, Some(workspace_root)) {
        return false;
    }
    // Symlink-tolerant relativization (see `relative_within_workspace`): a
    // deleted leaf cannot be canonicalized, so the helper strips via the
    // canonical root instead. If the path is genuinely outside the workspace we
    // still process the deletion (matches the prior strip-failure behavior).
    let rel_path = match crate::utils::paths::relative_within_workspace(path, workspace_root) {
        Some(p) => p,
        None => return true,
    };
    if gitignore
        .matched_path_or_any_parents(&rel_path, false)
        .is_ignore()
    {
        return false;
    }
    true
}
