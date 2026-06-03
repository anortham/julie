//! Glob pattern matching utilities.
//!
//! This module provides file-path glob matching used by search and analysis.
//! It supports comma/pipe-separated inclusion and exclusion patterns, brace
//! alternation groups (delegated to globset), and simple basename matching for
//! patterns that contain no wildcards or path separators.

use globset::{Glob, GlobMatcher};
use tracing::warn;

/// Match file path against a glob pattern expression.
///
/// A pattern expression is one or more glob segments separated by top-level
/// commas or pipes (separators inside `{...}` brace alternation groups stay
/// inside the group). Each segment is either an inclusion (`src/**`) or an
/// exclusion (`!docs/**`).
///
/// Match semantics:
/// `(inclusions.is_empty() || any(inclusions).matches(path)) && !any(exclusions).matches(path)`
///
/// An exclusion-only expression (`!docs/**`) implies "include everything except the
/// excluded set". Whitespace is NOT a segment separator — a pattern like
/// `**/file name.rs` is preserved as a single glob with a literal space.
pub fn matches_glob_pattern(file_path: &str, pattern: &str) -> bool {
    compile_patterns(pattern).matches(file_path)
}

/// A compiled single-segment matcher.
///
/// Three variants, each with a distinct reason:
/// - `Simple`: no wildcards and no separators in the source pattern — match the
///   file's basename directly. Tolerates UNC paths where globset would otherwise
///   choke on backslashes.
/// - `Glob`: any pattern containing wildcards or separators — compiled via
///   globset after normalizing backslashes to forward slashes.
/// - `Never`: a pattern that failed to compile. Preserves the legacy fail-safe
///   where a malformed inclusion keeps files out and a malformed exclusion
///   doesn't exclude anything (both map to "this matcher matches nothing").
enum PatternMatcher {
    Simple(String),
    Glob(Box<GlobMatcher>),
    Never,
}

impl PatternMatcher {
    fn compile(segment: &str) -> Self {
        let is_simple_filename = !segment.contains('*')
            && !segment.contains('?')
            && !segment.contains('/')
            && !segment.contains('\\');
        if is_simple_filename {
            return PatternMatcher::Simple(segment.to_string());
        }

        let normalized = segment.replace('\\', "/");
        match Glob::new(&normalized) {
            Ok(glob) => PatternMatcher::Glob(Box::new(glob.compile_matcher())),
            Err(e) => {
                warn!("Invalid glob pattern '{}': {}", segment, e);
                PatternMatcher::Never
            }
        }
    }

    fn matches(&self, basename: &str, normalized_path: &str) -> bool {
        match self {
            PatternMatcher::Simple(name) => basename == name,
            PatternMatcher::Glob(g) => g.is_match(normalized_path),
            PatternMatcher::Never => false,
        }
    }
}

struct CompiledPatterns {
    inclusions: Vec<PatternMatcher>,
    exclusions: Vec<PatternMatcher>,
}

impl CompiledPatterns {
    fn matches(&self, file_path: &str) -> bool {
        let normalized = file_path.replace('\\', "/");
        let basename = file_path.rsplit(['/', '\\']).next().unwrap_or(file_path);

        let included = self.inclusions.is_empty()
            || self
                .inclusions
                .iter()
                .any(|m| m.matches(basename, &normalized));
        let excluded = self
            .exclusions
            .iter()
            .any(|m| m.matches(basename, &normalized));

        included && !excluded
    }
}

/// Split a pattern string on top-level commas only.
///
/// Commas inside `{...}` brace groups are preserved as part of the group so
/// globset's native alternation (`{src/**,tests/**}`) still compiles as a single
/// glob. Whitespace is NOT a separator — embedded spaces stay inside the segment
/// they belong to.
fn split_top_level_commas(pattern: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth: u32 = 0;

    for c in pattern.chars() {
        match c {
            '{' => {
                depth += 1;
                current.push(c);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            ',' | '|' if depth == 0 => {
                result.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    result.push(current);
    result
}

/// Split `pattern` into inclusion/exclusion matcher lists.
///
/// The both-empty branch is unreachable when callers normalize empty or
/// whitespace-only `file_pattern` to `None` (see `execute_search`). If a caller
/// slips through with a malformed expression (e.g., `","` or `"!"`) we log and
/// fall back to a sentinel that matches nothing — avoids a production panic
/// while still surfacing the problem.
fn compile_patterns(pattern: &str) -> CompiledPatterns {
    let mut inclusions = Vec::new();
    let mut exclusions = Vec::new();

    for segment in split_top_level_commas(pattern) {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(exclusion_body) = trimmed.strip_prefix('!') {
            let body = exclusion_body.trim();
            if body.is_empty() {
                continue;
            }
            exclusions.push(PatternMatcher::compile(body));
        } else {
            inclusions.push(PatternMatcher::compile(trimmed));
        }
    }

    if inclusions.is_empty() && exclusions.is_empty() {
        warn!(
            "file_pattern {:?} yielded no valid segments; treating as match-nothing. \
             Callers should normalize empty/whitespace patterns to None before this point.",
            pattern
        );
        inclusions.push(PatternMatcher::Never);
    }

    CompiledPatterns {
        inclusions,
        exclusions,
    }
}

#[cfg(test)]
mod tests {
    use super::matches_glob_pattern;

    #[test]
    fn test_doublestar_recursive_match() {
        assert!(matches_glob_pattern("src/tools/search/query.rs", "**/query.rs"));
        assert!(!matches_glob_pattern("src/tools/search/query.rs", "**/mod.rs"));
    }

    #[test]
    fn test_comma_separated_inclusions() {
        assert!(matches_glob_pattern("src/lib.rs", "src/**,tests/**"));
        assert!(matches_glob_pattern("tests/foo.rs", "src/**,tests/**"));
        assert!(!matches_glob_pattern("docs/foo.md", "src/**,tests/**"));
    }

    #[test]
    fn test_exclusion_pattern() {
        // Include everything except docs/
        assert!(matches_glob_pattern("src/main.rs", "!docs/**"));
        assert!(!matches_glob_pattern("docs/guide.md", "!docs/**"));
    }

    #[test]
    fn test_simple_basename_match() {
        assert!(matches_glob_pattern("some/deep/path/mod.rs", "mod.rs"));
        assert!(!matches_glob_pattern("some/deep/path/lib.rs", "mod.rs"));
    }

    #[test]
    fn test_pipe_separator() {
        assert!(matches_glob_pattern("src/foo.rs", "src/**|tests/**"));
        assert!(matches_glob_pattern("tests/bar.rs", "src/**|tests/**"));
        assert!(!matches_glob_pattern("docs/baz.md", "src/**|tests/**"));
    }
}
