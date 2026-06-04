use crate::file_policy::{should_index_path_candidate, supported_extensions_for_indexing};
use crate::paths::to_relative_unix_style;
use crate::walk::{WalkConfig, build_walker};
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

/// Scan workspace and return a set of all code file paths (relative to workspace root).
///
/// Used to detect new files that aren't in the database yet, and to power
/// watcher repair scans. All returned paths use Unix-style forward slashes for
/// cross-platform compatibility with the database storage format.
pub fn scan_workspace_files(workspace_root: &Path) -> Result<HashSet<String>> {
    let mut files = HashSet::new();

    for result in build_walker(workspace_root, &WalkConfig::stale_scan()) {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        if !is_code_file(entry.path()) {
            continue;
        }

        // Get relative path from workspace root in Unix-style format
        // CRITICAL: Use to_relative_unix_style() to ensure cross-platform compatibility
        // On Windows, strip_prefix() returns paths with backslashes (src\file.rs)
        // But database stores paths with forward slashes (src/file.rs)
        if let Ok(relative_path) = to_relative_unix_style(entry.path(), workspace_root) {
            files.insert(relative_path);
        }
    }

    Ok(files)
}

/// Check if a file is a supported code file.
///
/// Accepts files through the same candidate policy used by watcher events:
/// known parser-backed extensions are included, blacklisted names/extensions
/// are rejected, and unknown or extensionless text files stay indexable as
/// text-only files. The goal is to keep startup freshness scans, overflow
/// repair scans, and live watcher events from disagreeing about tracked files.
pub fn is_code_file(path: &Path) -> bool {
    should_index_path_candidate(path, supported_extensions_for_indexing())
}
