//! Query processing and matching utilities
//!
//! Handles query preprocessing, glob pattern matching, and line matching strategies
//! for both file-level and line-level searching.

use globset::{Glob, GlobMatcher};
use tracing::warn;

use super::types::LineMatchStrategy;

/// Match file path against a glob pattern expression.
///
/// A pattern expression is one or more glob segments separated by top-level commas
/// (commas inside `{...}` brace alternation groups stay inside the group). Each
/// segment is either an inclusion (`src/**`) or an exclusion (`!docs/**`).
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
            ',' if depth == 0 => {
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
