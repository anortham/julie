//! File filtering logic for watcher operations
//!
//! This module provides utilities for determining which files should be indexed
//! based on extension and ignore patterns.

use crate::tools::shared::{BLACKLISTED_DIRECTORIES, BLACKLISTED_FILENAMES};
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

/// Build a gitignore-based matcher that layers:
/// 1. `.gitignore` patterns (if present in workspace root)
/// 2. `.julieignore` patterns (if present in workspace root)
/// 3. Synthetic patterns for Julie's own directories and common noise
pub fn build_gitignore_matcher(workspace_root: &Path) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(workspace_root);

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
    let check_path = workspace_root
        .and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path);
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
    let rel_path = match path.strip_prefix(workspace_root) {
        Ok(p) => p,
        Err(_) => return false,
    };
    gitignore
        .matched_path_or_any_parents(rel_path, path.is_dir())
        .is_ignore()
}

/// Check if a file should be indexed based on extension, blacklists, and gitignore.
///
/// Layers (in order):
/// 1. Must be an existing file on disk
/// 2. Filename must not be blacklisted (lockfiles, etc.)
/// 3. Extension must be in supported set
/// 4. No path component may be a blacklisted directory
/// 5. Must not match gitignore/julieignore/synthetic patterns
pub fn should_index_file(
    path: &Path,
    supported_extensions: &HashSet<String>,
    gitignore: &Gitignore,
    workspace_root: &Path,
) -> bool {
    if !path.is_file() {
        return false;
    }
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if BLACKLISTED_FILENAMES.contains(&file_name) {
            return false;
        }
    }
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if !supported_extensions.contains(ext) {
            return false;
        }
    } else {
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
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if BLACKLISTED_FILENAMES.contains(&file_name) {
            return false;
        }
    }
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if !supported_extensions.contains(ext) {
            return false;
        }
    } else {
        return false;
    }
    if contains_blacklisted_directory_relative(path, Some(workspace_root)) {
        return false;
    }
    let rel_path = match path.strip_prefix(workspace_root) {
        Ok(p) => p,
        Err(_) => return true,
    };
    if gitignore
        .matched_path_or_any_parents(rel_path, false)
        .is_ignore()
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_extensions() {
        let extensions = build_supported_extensions();
        assert!(extensions.contains("rs"));
        assert!(extensions.contains("ts"));
        assert!(extensions.contains("py"));
        assert!(!extensions.contains("txt"));
    }

    #[test]
    fn test_should_index_file_skips_lockfiles() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let lockfile = root.join("pnpm-lock.yaml");
        fs::write(&lockfile, "lockfileVersion: '9.0'").unwrap();

        let extensions = build_supported_extensions();
        let gitignore = build_gitignore_matcher(root).unwrap();

        assert!(
            !should_index_file(&lockfile, &extensions, &gitignore, root),
            "pnpm-lock.yaml must not be indexed by watcher"
        );
        let lockfile2 = root.join("package-lock.json");
        fs::write(&lockfile2, "{}").unwrap();
        assert!(
            !should_index_file(&lockfile2, &extensions, &gitignore, root),
            "package-lock.json must not be indexed by watcher"
        );
    }

    #[test]
    fn test_build_gitignore_matcher_with_gitignore_file() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Write a .gitignore with patterns and a negation
        fs::write(
            root.join(".gitignore"),
            "node_modules/\ntarget/\n!target/keep.rs\n",
        )
        .unwrap();

        let matcher = build_gitignore_matcher(root).unwrap();

        // .gitignore patterns
        assert!(
            matcher
                .matched_path_or_any_parents("node_modules/foo.js", false)
                .is_ignore(),
            "node_modules should be ignored"
        );
        assert!(
            matcher
                .matched_path_or_any_parents("target/debug/build.rs", false)
                .is_ignore(),
            "target dir should be ignored"
        );

        // Negation: !target/keep.rs should whitelist it
        assert!(
            !matcher
                .matched_path_or_any_parents("target/keep.rs", false)
                .is_ignore(),
            "negated pattern should not be ignored"
        );

        // Directory patterns from .gitignore
        assert!(
            matcher
                .matched_path_or_any_parents("node_modules/package/index.js", false)
                .is_ignore(),
            "nested under ignored directory"
        );

        // Synthetic patterns
        assert!(
            matcher
                .matched_path_or_any_parents(".julie/logs/test.log", false)
                .is_ignore(),
            ".julie/ synthetic should be ignored"
        );
        assert!(
            matcher
                .matched_path_or_any_parents(".memories/checkpoint.md", false)
                .is_ignore(),
            ".memories/ synthetic should be ignored"
        );
        assert!(
            matcher
                .matched_path_or_any_parents("cmake-build-debug/CMakeCache.txt", false)
                .is_ignore(),
            "cmake-build-* synthetic should be ignored"
        );
        assert!(
            matcher
                .matched_path_or_any_parents("dist/app.min.js", false)
                .is_ignore(),
            "*.min.js synthetic should be ignored"
        );

        // Non-ignored path
        assert!(
            !matcher
                .matched_path_or_any_parents("src/main.rs", false)
                .is_ignore(),
            "normal source file should not be ignored"
        );
    }

    #[test]
    fn test_build_gitignore_matcher_no_gitignore_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // No .gitignore or .julieignore written

        let matcher = build_gitignore_matcher(root).unwrap();

        // Synthetic patterns still work
        assert!(
            matcher
                .matched_path_or_any_parents(".julie/db/symbols.db", false)
                .is_ignore(),
            ".julie/ should be ignored via synthetics"
        );
        assert!(
            matcher
                .matched_path_or_any_parents("lib/app.bundle.js", false)
                .is_ignore(),
            "*.bundle.js should be ignored via synthetics"
        );

        // Normal files pass through
        assert!(
            !matcher
                .matched_path_or_any_parents("src/lib.rs", false)
                .is_ignore(),
            "normal source files should not be ignored"
        );
        assert!(
            !matcher
                .matched_path_or_any_parents("node_modules/foo.js", false)
                .is_ignore(),
            "without .gitignore, node_modules is NOT ignored"
        );
    }

    #[test]
    fn test_build_gitignore_matcher_merges_julieignore() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // .gitignore ignores node_modules
        fs::write(root.join(".gitignore"), "node_modules/\n").unwrap();
        // .julieignore adds custom vendor pattern
        fs::write(root.join(".julieignore"), "generated/\ndata/*.csv\n").unwrap();

        let matcher = build_gitignore_matcher(root).unwrap();

        // .gitignore pattern
        assert!(
            matcher
                .matched_path_or_any_parents("node_modules/foo.js", false)
                .is_ignore(),
            ".gitignore patterns should apply"
        );

        // .julieignore patterns
        assert!(
            matcher
                .matched_path_or_any_parents("generated/output.rs", false)
                .is_ignore(),
            ".julieignore generated/ should be ignored"
        );
        assert!(
            matcher
                .matched_path_or_any_parents("data/users.csv", false)
                .is_ignore(),
            ".julieignore data/*.csv should be ignored"
        );

        // Synthetic patterns still present
        assert!(
            matcher
                .matched_path_or_any_parents(".julie/logs/test.log", false)
                .is_ignore(),
            "synthetic patterns should still work"
        );

        // Unmatched files pass through
        assert!(
            !matcher
                .matched_path_or_any_parents("src/main.rs", false)
                .is_ignore(),
            "normal files should not be ignored"
        );
    }

    #[test]
    fn test_contains_blacklisted_directory() {
        // Blacklisted directories
        assert!(contains_blacklisted_directory(Path::new(
            "/repo/node_modules/foo/bar.js"
        )));
        assert!(contains_blacklisted_directory(Path::new(
            "/repo/src/.git/config"
        )));
        assert!(contains_blacklisted_directory(Path::new(
            "/repo/target/debug/build.rs"
        )));
        assert!(contains_blacklisted_directory(Path::new(
            "/repo/obj/Debug/net9.0/app.dll"
        )));
        assert!(contains_blacklisted_directory(Path::new(
            "/repo/bin/Release/app.exe"
        )));
        assert!(contains_blacklisted_directory(Path::new(
            "/repo/__pycache__/module.pyc"
        )));
        assert!(contains_blacklisted_directory(Path::new(
            "/repo/.idea/workspace.xml"
        )));

        // Non-blacklisted directories
        assert!(!contains_blacklisted_directory(Path::new(
            "/repo/src/main.rs"
        )));
        assert!(!contains_blacklisted_directory(Path::new(
            "/repo/lib/utils.py"
        )));
        assert!(!contains_blacklisted_directory(Path::new(
            "/repo/packages/core/index.ts"
        )));
    }

    #[test]
    fn test_is_gitignored() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        fs::write(root.join(".gitignore"), "build/\n*.log\n").unwrap();
        let matcher = build_gitignore_matcher(root).unwrap();

        // Ignored by .gitignore
        assert!(is_gitignored(&root.join("build/output.js"), &matcher, root));
        assert!(is_gitignored(&root.join("debug.log"), &matcher, root));
        assert!(is_gitignored(&root.join("nested/deep.log"), &matcher, root));

        // Not ignored
        assert!(!is_gitignored(&root.join("src/main.rs"), &matcher, root));
        assert!(!is_gitignored(&root.join("lib/utils.py"), &matcher, root));

        // Path outside workspace root returns false (not ignored)
        assert!(!is_gitignored(
            Path::new("/completely/different/path.rs"),
            &matcher,
            root
        ));
    }

    #[test]
    fn test_should_index_file_with_gitignore() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create .gitignore
        fs::write(root.join(".gitignore"), "vendor/\n*.log\n").unwrap();

        // Create test files
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("vendor")).unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("vendor/lib.rs"), "// vendor").unwrap();
        fs::write(root.join("node_modules/foo.js"), "// nm").unwrap();
        fs::write(root.join("debug.log"), "log line").unwrap();
        fs::write(root.join("src/app.txt"), "not code").unwrap();
        fs::create_dir_all(root.join(".julie/db")).unwrap();
        fs::write(root.join(".julie/db/test.rs"), "// julie data").unwrap();

        let extensions = build_supported_extensions();
        let gitignore = build_gitignore_matcher(root).unwrap();

        // Normal source file — accepted
        assert!(should_index_file(
            &root.join("src/main.rs"),
            &extensions,
            &gitignore,
            root
        ));

        // Gitignored vendor/ — rejected
        assert!(!should_index_file(
            &root.join("vendor/lib.rs"),
            &extensions,
            &gitignore,
            root
        ));

        // Blacklisted directory node_modules/ — rejected
        assert!(!should_index_file(
            &root.join("node_modules/foo.js"),
            &extensions,
            &gitignore,
            root
        ));

        // Unsupported extension — rejected
        assert!(!should_index_file(
            &root.join("src/app.txt"),
            &extensions,
            &gitignore,
            root
        ));

        // Synthetic pattern .julie/ — rejected
        assert!(!should_index_file(
            &root.join(".julie/db/test.rs"),
            &extensions,
            &gitignore,
            root
        ));
    }

    #[test]
    fn test_should_process_deletion_with_gitignore() {
        use std::fs;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        fs::write(root.join(".gitignore"), "build/\n").unwrap();

        let extensions = build_supported_extensions();
        let gitignore = build_gitignore_matcher(root).unwrap();

        // Normal source file (doesn't exist on disk — that's fine for deletion)
        assert!(should_process_deletion(
            &root.join("src/main.rs"),
            &extensions,
            &gitignore,
            root
        ));

        // Gitignored path — skip deletion processing
        assert!(!should_process_deletion(
            &root.join("build/output.rs"),
            &extensions,
            &gitignore,
            root
        ));

        // Blacklisted directory — skip
        assert!(!should_process_deletion(
            &root.join("node_modules/foo.js"),
            &extensions,
            &gitignore,
            root
        ));

        // Blacklisted filename — skip
        assert!(!should_process_deletion(
            &root.join("pnpm-lock.yaml"),
            &extensions,
            &gitignore,
            root
        ));

        // Unsupported extension — skip
        assert!(!should_process_deletion(
            &root.join("readme.txt"),
            &extensions,
            &gitignore,
            root
        ));

        // Path outside workspace root — should be processed (returns true)
        assert!(should_process_deletion(
            Path::new("/other/repo/main.rs"),
            &extensions,
            &gitignore,
            root
        ));
    }
}
