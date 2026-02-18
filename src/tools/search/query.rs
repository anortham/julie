//! Query processing and matching utilities
//!
//! Handles query preprocessing, glob pattern matching, and line matching strategies
//! for both file-level and line-level searching.

use globset::Glob;
use tracing::warn;

use super::types::LineMatchStrategy;

/// Match file path against glob pattern (supports exclusions with !)
/// Uses globset crate for proper glob matching instead of fragile string contains()
pub fn matches_glob_pattern(file_path: &str, pattern: &str) -> bool {
    // Handle exclusion patterns (starting with !)
    if let Some(exclusion_pattern) = pattern.strip_prefix('!') {
        // Exclusion: return true if path does NOT match the pattern
        match Glob::new(exclusion_pattern) {
            Ok(glob) => {
                let matcher = glob.compile_matcher();
                !matcher.is_match(file_path)
            }
            Err(e) => {
                warn!(
                    "Invalid exclusion glob pattern '{}': {}",
                    exclusion_pattern, e
                );
                true // On error, don't exclude
            }
        }
    } else {
        // Inclusion: return true if path matches the pattern

        // Special case: Simple filename patterns (no wildcards, no path separators)
        // Match against basename only, not full UNC path
        // Example: "Program.cs" should match "\\?\C:\source\project\Program.cs"
        let is_simple_filename = !pattern.contains('*')
            && !pattern.contains('?')
            && !pattern.contains('/')
            && !pattern.contains('\\');

        if is_simple_filename {
            // Extract basename from path (handle both / and \ separators, including UNC paths)
            let basename = file_path.rsplit(['/', '\\']).next().unwrap_or(file_path);

            // Simple string comparison for exact filename match
            return basename == pattern;
        }

        // Standard glob matching for patterns with wildcards or path separators
        // CRITICAL: Normalize Windows paths to Unix-style for glob matching
        // The glob crate expects forward slashes, but Windows paths use backslashes
        // Example: \\?\C:\source\project\file.rs → //?/C:/source/project/file.rs
        let normalized_path = file_path.replace('\\', "/");
        let normalized_pattern = pattern.replace('\\', "/");

        match Glob::new(&normalized_pattern) {
            Ok(glob) => {
                let matcher = glob.compile_matcher();
                // Match against normalized path (forward slashes)
                matcher.is_match(&normalized_path)
            }
            Err(e) => {
                warn!("Invalid glob pattern '{}': {}", pattern, e);
                false // On error, don't match
            }
        }
    }
}

/// Create line match strategy from a query string
///
/// Determines whether to use substring matching or token-based matching
/// with support for exclusions using '-' prefix.
pub fn line_match_strategy(query: &str) -> LineMatchStrategy {
    let trimmed = query.trim();

    if trimmed.is_empty()
        || trimmed.contains('"')
        || trimmed.contains('\'')
        || trimmed.contains('*')
        || trimmed.contains(" AND ")
        || trimmed.contains(" OR ")
        || trimmed.contains(" NOT ")
    {
        return LineMatchStrategy::Substring(trimmed.to_lowercase());
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();

    // Single word (possibly compound like files_by_language) → substring match
    if words.len() == 1 {
        return LineMatchStrategy::Substring(trimmed.to_lowercase());
    }

    // Multi-word: check for exclusion tokens
    let mut required = Vec::new();
    let mut excluded = Vec::new();

    for word in &words {
        if word.starts_with('-') && word.len() > 1 {
            excluded.push(word[1..].to_lowercase());
        } else if !word.is_empty() {
            required.push(word.to_lowercase());
        }
    }

    // If there are exclusions, use Tokens strategy (same-line AND with exclusions)
    if !excluded.is_empty() {
        return LineMatchStrategy::Tokens { required, excluded };
    }

    // Multi-word without exclusions → FileLevel (cross-line OR, Tantivy guarantees file-level AND)
    if required.is_empty() {
        LineMatchStrategy::Substring(trimmed.to_lowercase())
    } else {
        LineMatchStrategy::FileLevel { terms: required }
    }
}

/// Check if a line matches the given strategy
pub fn line_matches(strategy: &LineMatchStrategy, line: &str) -> bool {
    let line_lower = line.to_lowercase();

    match strategy {
        LineMatchStrategy::Substring(pattern) => {
            !pattern.is_empty() && line_lower.contains(pattern)
        }
        LineMatchStrategy::Tokens { required, excluded } => {
            let required_ok =
                required.is_empty() || required.iter().all(|token| line_lower.contains(token));
            let excluded_ok = excluded.iter().all(|token| !line_lower.contains(token));
            required_ok && excluded_ok
        }
        LineMatchStrategy::FileLevel { terms } => {
            // Match lines containing ANY term (OR logic)
            // Tantivy already guarantees all terms exist in the file
            terms.iter().any(|term| line_lower.contains(term))
        }
    }
}
