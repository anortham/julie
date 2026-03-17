//! File filtering logic for watcher operations
//!
//! This module provides utilities for determining which files should be indexed
//! based on extension and ignore patterns.

use crate::tools::shared::BLACKLISTED_FILENAMES;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

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
        "**/.worktrees/**",  // Git worktrees (separate working contexts)
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
}
