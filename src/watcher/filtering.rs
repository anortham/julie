//! File filtering logic for watcher operations
//!
//! This module provides utilities for determining which files should be indexed
//! based on extension and ignore patterns.

use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

/// Build set of supported file extensions
pub fn build_supported_extensions() -> HashSet<String> {
    [
        "rs", "ts", "tsx", "js", "jsx", "py", "java", "cs", "cpp", "cxx", "cc", "c", "h", "go",
        "php", "rb", "swift", "kt", "lua", "gd", "sql", "html", "htm", "css", "vue", "razor",
        "ps1", "sh", "bash", "qml", "zig", "dart", "r", "R",
        // Documentation and config files (extractors #28-30)
        "md", "markdown", "json", "jsonl", "toml", "yml", "yaml",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Build ignore patterns for files/directories to skip
pub fn build_ignore_patterns() -> Result<Vec<glob::Pattern>> {
    let patterns = [
        "**/node_modules/**",
        "**/target/**",
        "**/build/**",
        "**/dist/**",
        "**/.git/**",
        "**/.julie/**", // Don't watch our own data directory
        "**/*.min.js",
        "**/*.bundle.js",
        "**/*.map",
        "**/coverage/**",
        "**/.nyc_output/**",
        "**/tmp/**",
        "**/temp/**",
        "**/__pycache__/**",
        "**/*.pyc",
        "**/vendor/**",
        "**/node_modules.nosync/**",
    ];

    patterns
        .iter()
        .map(|p| {
            glob::Pattern::new(p)
                .map_err(|e| anyhow::anyhow!("Invalid glob pattern {}: {}", p, e))
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
}
