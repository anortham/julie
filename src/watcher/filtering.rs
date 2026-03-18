//! File filtering logic for watcher operations
//!
//! This module provides utilities for determining which files should be indexed
//! based on extension and ignore patterns.

use crate::tools::shared::BLACKLISTED_FILENAMES;
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

/// Build ignore patterns for files/directories to skip
pub fn build_ignore_patterns() -> Result<Vec<glob::Pattern>> {
    let patterns = [
        // Package managers and dependencies
        "**/node_modules/**",
        "**/vendor/**",
        "**/node_modules.nosync/**",
        // Build outputs (language-specific)
        "**/target/**",        // Rust
        "**/build/**",         // Generic builds
        "**/dist/**",          // Distribution builds
        "**/out/**",           // Generic output (Java, Kotlin, JetBrains IDEs)
        "**/obj/**",           // .NET intermediate outputs
        "**/bin/**",           // .NET final outputs
        "**/.gradle/**",       // Gradle build cache (Java, Android)
        "**/.dart_tool/**",    // Dart/Flutter build cache
        "**/cmake-build-*/**", // CMake build directories (cmake-build-debug, cmake-build-release, etc.)
        // JavaScript/TypeScript framework caches
        "**/.next/**", // Next.js build cache
        "**/.nuxt/**", // Nuxt.js build cache
        // Version control
        "**/.git/**",
        "**/.worktrees/**",        // Git worktrees (separate working contexts)
        "**/.claude/worktrees/**", // Claude Code agent worktrees (temporary checkouts)
        // Julie's own data
        "**/.julie/**",    // Don't watch our own data directory
        "**/.memories/**", // Goldfish memory files (not code)
        // Minified/bundled files
        "**/*.min.js",
        "**/*.bundle.js",
        "**/*.map",
        // Test coverage
        "**/coverage/**",
        "**/.nyc_output/**",
        // Temporary files
        "**/tmp/**",
        "**/temp/**",
        // Python
        "**/__pycache__/**",
        "**/*.pyc",
    ];

    patterns
        .iter()
        .map(|p| {
            glob::Pattern::new(p).map_err(|e| anyhow::anyhow!("Invalid glob pattern {}: {}", p, e))
        })
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
            warn!("Partial error reading {}: {}", gitignore_path.display(), err);
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

/// Check if a file should be indexed based on extension and ignore patterns
#[allow(dead_code)]
pub fn should_index_file(
    path: &Path,
    supported_extensions: &HashSet<String>,
    ignore_patterns: &[glob::Pattern],
) -> bool {
    // Check if it's a file
    if !path.is_file() {
        return false;
    }

    // Skip blacklisted filenames (lockfiles with non-blacklisted extensions)
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if BLACKLISTED_FILENAMES.contains(&file_name) {
            return false;
        }
    }

    // Check extension
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        if !supported_extensions.contains(ext) {
            return false;
        }
    } else {
        return false; // No extension
    }

    // Check ignore patterns
    let path_str = path.to_string_lossy();
    for pattern in ignore_patterns {
        if pattern.matches(&path_str) {
            return false;
        }
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
    fn test_ignore_patterns() {
        let patterns = build_ignore_patterns().unwrap();
        assert!(!patterns.is_empty());

        let node_modules_pattern = patterns
            .iter()
            .find(|p| p.as_str().contains("node_modules"))
            .expect("Should have node_modules pattern");

        assert!(node_modules_pattern.matches("src/node_modules/package.json"));
    }

    #[test]
    fn test_dotnet_build_artifacts_ignored() {
        let patterns = build_ignore_patterns().unwrap();

        // Test obj/ directory (intermediate build outputs)
        let obj_match = patterns.iter().any(|p| {
            p.matches("MyProject/obj/Debug/net9.0/MyProject.dll")
                || p.matches("src/obj/Debug/MyProject.json")
                || p.matches("obj/staticwebassets.build.json")
        });
        assert!(obj_match, ".NET obj/ directories should be ignored");

        // Test bin/ directory (final build outputs)
        let bin_match = patterns.iter().any(|p| {
            p.matches("MyProject/bin/Debug/net9.0/MyProject.dll")
                || p.matches("src/bin/Release/MyProject.json")
                || p.matches("bin/wwwroot/framework/blazor.boot.json")
        });
        assert!(bin_match, ".NET bin/ directories should be ignored");
    }

    #[test]
    fn test_additional_build_artifacts_ignored() {
        let patterns = build_ignore_patterns().unwrap();

        // Test .gradle/ (Java/Android builds)
        let gradle_match = patterns.iter().any(|p| {
            p.matches("MyApp/.gradle/7.5/checksums/checksums.lock")
                || p.matches(".gradle/buildOutputCleanup/cache.properties")
                || p.matches("android/.gradle/file-system.probe")
        });
        assert!(gradle_match, "Gradle build directories should be ignored");

        // Test .dart_tool/ (Dart/Flutter)
        let dart_match = patterns.iter().any(|p| {
            p.matches("my_flutter_app/.dart_tool/package_config.json")
                || p.matches(".dart_tool/version")
        });
        assert!(dart_match, "Dart tool directories should be ignored");

        // Test .next/ (Next.js build cache)
        let next_match = patterns.iter().any(|p| {
            p.matches("my-nextjs-app/.next/cache/webpack/client-production/0.pack")
                || p.matches(".next/build-manifest.json")
        });
        assert!(next_match, "Next.js build cache should be ignored");

        // Test .nuxt/ (Nuxt.js build cache)
        let nuxt_match = patterns.iter().any(|p| {
            p.matches("my-nuxt-app/.nuxt/dist/server/index.js") || p.matches(".nuxt/routes.json")
        });
        assert!(nuxt_match, "Nuxt.js build cache should be ignored");

        // Test cmake-build-* (CMake build directories)
        let cmake_match = patterns.iter().any(|p| {
            p.matches("cmake-build-debug/CMakeCache.txt")
                || p.matches("cmake-build-release/Makefile")
                || p.matches("project/cmake-build-relwithdebinfo/compile_commands.json")
        });
        assert!(cmake_match, "CMake build directories should be ignored");

        // Test out/ (Generic output directories - common in JetBrains IDEs)
        let out_match = patterns.iter().any(|p| {
            p.matches("MyProject/out/production/MyProject/Main.class")
                || p.matches("out/artifacts/MyApp.jar")
        });
        assert!(out_match, "Generic output directories should be ignored");
    }

    #[test]
    fn test_should_index_file_skips_lockfiles() {
        use std::fs;
        let dir = std::env::temp_dir();
        let lockfile = dir.join("pnpm-lock.yaml");
        fs::write(&lockfile, "lockfileVersion: '9.0'").unwrap();

        let extensions = build_supported_extensions();
        let patterns = build_ignore_patterns().unwrap();

        assert!(
            !should_index_file(&lockfile, &extensions, &patterns),
            "pnpm-lock.yaml must not be indexed by watcher"
        );
        let lockfile2 = dir.join("package-lock.json");
        fs::write(&lockfile2, "{}").unwrap();
        assert!(
            !should_index_file(&lockfile2, &extensions, &patterns),
            "package-lock.json must not be indexed by watcher"
        );

        fs::remove_file(&lockfile).ok();
        fs::remove_file(&lockfile2).ok();
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
}
