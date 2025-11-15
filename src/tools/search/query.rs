//! Query processing and matching utilities
//!
//! Handles query preprocessing, glob pattern matching, and line matching strategies
//! for both file-level and line-level searching.

use globset::Glob;
use tracing::warn;

use super::types::LineMatchStrategy;

/// FTS5 Query Preprocessor - Extract positive terms only, filter exclusions in application
///
/// **CRITICAL ARCHITECTURAL DECISION**:
/// FTS5 searches at FILE level, not LINE level. When a file contains both "user" and "password",
/// the query `user NOT password` returns ZERO files (file contains both terms).
///
/// **Correct approach for line-level search**:
/// 1. FTS5 query: Only positive terms ("user") → finds files containing "user"
/// 2. Application filtering: `line_match_strategy` excludes lines containing "password"
///
/// This is NOT a hack - this is the ONLY correct way to do line-level exclusion with file-level FTS5.
pub fn preprocess_fallback_query(query: &str) -> String {
    let trimmed = query.trim();

    // If already quoted, pass through as-is
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        return trimmed.to_string();
    }

    // If query contains explicit FTS5 operators, pass through
    if trimmed.contains(" NOT ") || trimmed.contains(" AND ") || trimmed.contains(" OR ") {
        return trimmed.to_string();
    }

    // If query contains wildcard, pass through
    if trimmed.contains('*') {
        return trimmed.to_string();
    }

    // Extract ONLY positive terms for FTS5 file-level search
    // Exclusions handled by line_match_strategy after retrieving file content
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    let positive_terms: Vec<&str> = words
        .into_iter()
        .filter(|w| !w.starts_with('-') || w.len() == 1)
        .collect();

    if positive_terms.is_empty() {
        // No positive terms - return original (FTS5 will handle error)
        return trimmed.to_string();
    }

    positive_terms.join(" ")
}

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

    let mut required = Vec::new();
    let mut excluded = Vec::new();

    for token in trimmed.split_whitespace() {
        if token.starts_with('-') && token.len() > 1 {
            excluded.push(token[1..].to_lowercase());
        } else if !token.is_empty() {
            required.push(token.to_lowercase());
        }
    }

    if required.is_empty() && excluded.is_empty() {
        LineMatchStrategy::Substring(trimmed.to_lowercase())
    } else {
        LineMatchStrategy::Tokens { required, excluded }
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
    }
}
