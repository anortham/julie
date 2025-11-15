//! Utilities for handling .julieignore file patterns
//!
//! This module provides shared functionality for loading and matching .julieignore patterns,
//! ensuring consistent ignore behavior across discovery and startup scanning.
//!
use anyhow::Result;
use std::fs;
use std::path::Path;
use tracing::debug;

/// Load custom ignore patterns from .julieignore file in workspace root
///
/// Returns a vector of patterns to ignore. Empty lines and comments (lines starting with #) are skipped.
///
/// # Examples
///
/// ```text
/// # .julieignore file content
/// generated/
/// *.min.js
/// temp_files/
/// ```
pub fn load_julieignore(workspace_path: &Path) -> Result<Vec<String>> {
    let ignore_file = workspace_path.join(".julieignore");

    if !ignore_file.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&ignore_file)
        .map_err(|e| anyhow::anyhow!("Failed to read .julieignore: {}", e))?;

    let patterns: Vec<String> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect();

    if !patterns.is_empty() {
        debug!(
            "📋 Loaded {} custom ignore patterns from .julieignore",
            patterns.len()
        );
    }

    Ok(patterns)
}

/// Check if a path matches any of the custom ignore patterns
///
/// Supports three pattern types with proper word boundary handling:
/// - Directory patterns (ending with /): matches directory name as whole word, plus all contents
/// - Wildcard extension patterns (starting with *.): matches file extension
/// - Substring patterns: matches anywhere in path
///
/// Word boundary fix: "packages/" matches "packages" and "src/packages" but NOT
/// "my-packages" or "subpackages" (prevents false positives).
///
/// # Examples
///
/// ```rust
/// use std::path::Path;
/// use julie::utils::ignore::is_ignored_by_pattern;
///
/// let patterns = vec!["generated/".to_string(), "*.min.js".to_string(), "temp".to_string()];
/// let path1 = Path::new("/project/generated/schema.rs");
/// let path2 = Path::new("/project/src/app.min.js");
/// let path3 = Path::new("/project/temp_files/data.txt");
///
/// assert!(is_ignored_by_pattern(path1, &patterns));
/// assert!(is_ignored_by_pattern(path2, &patterns));
/// assert!(is_ignored_by_pattern(path3, &patterns));
/// ```
pub fn is_ignored_by_pattern(path: &Path, patterns: &[String]) -> bool {
    if patterns.is_empty() {
        return false;
    }

    // Normalize path to Unix-style for consistent pattern matching
    // On Windows, paths use backslashes, but .julieignore patterns use forward slashes
    let path_str = path.to_str().unwrap_or("").replace('\\', "/");

    for pattern in patterns {
        // Directory pattern (ends with /)
        if pattern.ends_with('/') {
            let dir_name = &pattern[..pattern.len() - 1];

            // Check if pattern matches with proper word boundaries:
            // 1. Full pattern for files within directory (e.g., "packages/" in ".../packages/file.js")
            if path_str.contains(pattern) {
                return true;
            }

            // 2. Directory at end of path with word boundary check
            //    Match "packages" in "src/packages" but NOT in "my-packages"
            if path_str.ends_with(dir_name) {
                // Check word boundary: must be preceded by '/' or be at start of string
                let before_dir_name_pos = path_str.len() - dir_name.len();
                if before_dir_name_pos == 0 || path_str.as_bytes()[before_dir_name_pos - 1] == b'/'
                {
                    return true;
                }
            }

            // 3. Directory as path component (e.g., "/packages/" in path)
            let pattern_with_separators = format!("/{}/", dir_name);
            if path_str.contains(&pattern_with_separators) {
                return true;
            }
        }
        // Wildcard extension pattern (e.g., *.min.js)
        else if pattern.starts_with("*.") {
            let ext_pattern = &pattern[1..]; // Remove the *
            if path_str.ends_with(ext_pattern) {
                return true;
            }
        }
        // Substring match (matches anywhere in path)
        else if path_str.contains(pattern) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_load_empty_julieignore() {
        let temp_dir = TempDir::new().unwrap();
        let patterns = load_julieignore(temp_dir.path()).unwrap();
        assert!(
            patterns.is_empty(),
            "Should return empty vector if .julieignore doesn't exist"
        );
    }

    #[test]
    fn test_load_julieignore_with_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let julieignore_path = temp_dir.path().join(".julieignore");

        fs::write(
            &julieignore_path,
            "# Comment line\ngenerated/\n*.min.js\n\ntemp_files/\n# Another comment\n",
        )
        .unwrap();

        let patterns = load_julieignore(temp_dir.path()).unwrap();
        assert_eq!(
            patterns.len(),
            3,
            "Should load 3 patterns (ignoring comments and empty lines)"
        );
        assert!(patterns.contains(&"generated/".to_string()));
        assert!(patterns.contains(&"*.min.js".to_string()));
        assert!(patterns.contains(&"temp_files/".to_string()));
    }

    #[test]
    fn test_is_ignored_directory_pattern() {
        let patterns = vec!["generated/".to_string()];
        let path = PathBuf::from("/project/generated/schema.rs");
        assert!(
            is_ignored_by_pattern(&path, &patterns),
            "Should match directory pattern"
        );
    }

    #[test]
    fn test_is_ignored_wildcard_extension() {
        let patterns = vec!["*.min.js".to_string()];
        let path = PathBuf::from("/project/src/app.min.js");
        assert!(
            is_ignored_by_pattern(&path, &patterns),
            "Should match wildcard extension"
        );
    }

    #[test]
    fn test_is_ignored_substring_match() {
        let patterns = vec!["temp".to_string()];
        let path = PathBuf::from("/project/temp_files/data.txt");
        assert!(
            is_ignored_by_pattern(&path, &patterns),
            "Should match substring"
        );
    }

    #[test]
    fn test_not_ignored_when_no_match() {
        let patterns = vec!["generated/".to_string(), "*.min.js".to_string()];
        let path = PathBuf::from("/project/src/normal.rs");
        assert!(
            !is_ignored_by_pattern(&path, &patterns),
            "Should NOT match when no pattern matches"
        );
    }

    // ========== NEW TESTS FOR WORD BOUNDARY EDGE CASES ==========
    // These tests demonstrate the bugs we found in the code review

    #[test]
    fn test_directory_pattern_word_boundary_false_positives() {
        // Pattern "packages/" should match "packages" but NOT "my-packages", "subpackages", etc.
        let patterns = vec!["packages/".to_string()];

        // Should match (correct directory)
        assert!(
            is_ignored_by_pattern(&PathBuf::from("packages"), &patterns),
            "Should match exact directory name"
        );
        assert!(
            is_ignored_by_pattern(&PathBuf::from("src/packages"), &patterns),
            "Should match directory in path"
        );
        assert!(
            is_ignored_by_pattern(&PathBuf::from("packages/file.js"), &patterns),
            "Should match file within directory"
        );

        // Should NOT match (different directories that happen to end with "packages")
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("my-packages"), &patterns),
            "Should NOT match 'my-packages' (ends with 'packages' but different directory)"
        );
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("src/my-packages"), &patterns),
            "Should NOT match 'src/my-packages'"
        );
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("subpackages"), &patterns),
            "Should NOT match 'subpackages' (ends with 'packages' but different word)"
        );
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("packages-old"), &patterns),
            "Should NOT match 'packages-old' (starts with 'packages' but different directory)"
        );
    }

    #[test]
    fn test_node_modules_pattern_specificity() {
        // Real-world case: "node_modules/" should not match "my_node_modules"
        let patterns = vec!["node_modules/".to_string()];

        // Should match
        assert!(is_ignored_by_pattern(
            &PathBuf::from("node_modules"),
            &patterns
        ));
        assert!(is_ignored_by_pattern(
            &PathBuf::from("project/node_modules"),
            &patterns
        ));
        assert!(is_ignored_by_pattern(
            &PathBuf::from("node_modules/package/index.js"),
            &patterns
        ));

        // Should NOT match
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("my_node_modules"), &patterns),
            "Should NOT match 'my_node_modules'"
        );
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("old_node_modules"), &patterns),
            "Should NOT match 'old_node_modules'"
        );
    }

    #[test]
    fn test_bin_pattern_specificity() {
        // Real-world case: "bin/" should not match "ruby-bin" or "sbin"
        let patterns = vec!["bin/".to_string()];

        // Should match
        assert!(is_ignored_by_pattern(&PathBuf::from("bin"), &patterns));
        assert!(is_ignored_by_pattern(
            &PathBuf::from("project/bin"),
            &patterns
        ));

        // Should NOT match
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("ruby-bin"), &patterns),
            "Should NOT match 'ruby-bin'"
        );
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("/usr/sbin"), &patterns),
            "Should NOT match 'sbin'"
        );
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("bin-old"), &patterns),
            "Should NOT match 'bin-old'"
        );
    }

    #[test]
    fn test_obj_pattern_specificity() {
        // Real-world case: "obj/" should not match "config-obj" or "myobj"
        let patterns = vec!["obj/".to_string()];

        // Should match
        assert!(is_ignored_by_pattern(&PathBuf::from("obj"), &patterns));
        assert!(is_ignored_by_pattern(
            &PathBuf::from("project/obj"),
            &patterns
        ));

        // Should NOT match
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("config-obj"), &patterns),
            "Should NOT match 'config-obj'"
        );
        assert!(
            !is_ignored_by_pattern(&PathBuf::from("myobj"), &patterns),
            "Should NOT match 'myobj'"
        );
    }
}
