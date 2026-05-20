//! Query processing and matching utilities
//!
//! Handles query preprocessing, glob pattern matching, and line matching strategies
//! for both file-level and line-level searching.

use globset::{Glob, GlobMatcher};
use std::collections::HashSet;
use tantivy::tokenizer::{TokenStream, Tokenizer};
use tracing::warn;

use crate::search::tokenizer::CodeTokenizer;

use super::types::LineMatchStrategy;

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

/// Heuristic for the common caller mistake `a/** b/**`: multiple top-level
/// globs separated by whitespace instead of `,` or `|`.
///
/// We bias toward "only fire when likely" instead of trying to parse every
/// possible spaced filename. Literal-path patterns such as `**/file name.rs`
/// should not trip this detector.
pub fn looks_like_whitespace_separated_globs(pattern: &str) -> bool {
    let tokens: Vec<&str> = pattern.split_whitespace().collect();
    if tokens.len() < 2 {
        return false;
    }

    tokens
        .iter()
        .all(|token| token.starts_with('!') || token.contains('*') || token.contains('/'))
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
        || trimmed.contains(" NOT ")
    {
        return LineMatchStrategy::Substring(trimmed.to_string());
    }

    if let Some(terms) = clean_or_disjunction_terms(trimmed) {
        return LineMatchStrategy::FileLevel { terms };
    }

    if trimmed.contains(" OR ") {
        return LineMatchStrategy::Substring(trimmed.to_string());
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();

    // Single word (possibly compound like files_by_language) → substring match
    if words.len() == 1 {
        return LineMatchStrategy::Substring(trimmed.to_string());
    }

    // Multi-word: check for exclusion tokens
    let mut required = Vec::new();
    let mut excluded = Vec::new();

    for word in &words {
        if word.starts_with('-') && word.len() > 1 {
            excluded.push(word[1..].to_string());
        } else if !word.is_empty() {
            required.push(word.to_string());
        }
    }

    // If there are exclusions, use Tokens strategy (same-line AND with exclusions)
    if !excluded.is_empty() {
        return LineMatchStrategy::Tokens { required, excluded };
    }

    // Multi-word without exclusions → FileLevel (cross-line OR, Tantivy guarantees file-level AND)
    if required.is_empty() {
        LineMatchStrategy::Substring(trimmed.to_string())
    } else {
        LineMatchStrategy::FileLevel { terms: required }
    }
}

pub(crate) fn clean_or_disjunction_terms(query: &str) -> Option<Vec<String>> {
    let trimmed = query.trim();
    if !trimmed.contains(" OR ") {
        return None;
    }

    let branches: Vec<&str> = trimmed.split(" OR ").map(str::trim).collect();
    if branches.len() < 2 {
        return None;
    }

    if branches
        .iter()
        .any(|branch| branch.is_empty() || branch.split_whitespace().count() != 1)
    {
        return None;
    }

    if !branches
        .iter()
        .all(|branch| is_code_identifier_branch(branch))
    {
        return None;
    }

    Some(branches.iter().map(|branch| branch.to_string()).collect())
}

/// Keep the boolean OR heuristic narrow. Hyphenated terms are deliberately
/// rejected so kebab-case literals and CSS-like names stay on substring
/// matching unless we add a more explicit parser.
fn is_code_identifier_branch(branch: &str) -> bool {
    let mut has_code_signal = false;

    for ch in branch.chars() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':')) {
            return false;
        }
        if ch.is_ascii_lowercase() || matches!(ch, '_' | '-' | '.' | ':') {
            has_code_signal = true;
        }
    }

    has_code_signal
}

/// Check if a line matches the given strategy
pub fn line_matches(strategy: &LineMatchStrategy, line: &str) -> bool {
    match strategy {
        LineMatchStrategy::Substring(pattern) => {
            !pattern.is_empty() && line_matches_literal(line, pattern)
        }
        LineMatchStrategy::Tokens { required, excluded } => {
            let line_tokens = tokenize_text_for_line_match(line);
            let required_ok = required.is_empty()
                || required
                    .iter()
                    .all(|term| term_matches_line(term, line, &line_tokens));
            let excluded_ok = excluded
                .iter()
                .all(|term| !term_matches_line(term, line, &line_tokens));
            required_ok && excluded_ok
        }
        LineMatchStrategy::FileLevel { terms } => {
            // Match lines containing ANY term (OR logic)
            // Tantivy already guarantees all terms exist in the file
            let line_tokens = tokenize_text_for_line_match(line);
            terms
                .iter()
                .any(|term| term_matches_line(term, line, &line_tokens))
        }
    }
}

fn line_matches_literal(line: &str, pattern: &str) -> bool {
    if let Some(phrase) = strip_balanced_wrapping_quotes(pattern) {
        return line_matches_tokenized_phrase(line, phrase);
    }

    let line_lower = line.to_lowercase();
    let pattern_lower = pattern.to_lowercase();
    if line_lower.contains(&pattern_lower) {
        return true;
    }

    normalized_literal_patterns(pattern)
        .iter()
        .any(|variant| line_lower.contains(variant))
        || pattern.chars().any(|ch| ch.is_ascii_punctuation())
            && line_matches_punctuation_normalized_phrase(line, pattern)
}

fn normalized_literal_patterns(pattern: &str) -> Vec<String> {
    let mut variants = Vec::new();
    let pattern_lower = pattern.to_lowercase();

    // Always push _ ↔ - swapped lowercase variants.
    push_unique_variant(
        &mut variants,
        pattern_lower.replace('-', "_"),
        &pattern_lower,
    );
    push_unique_variant(
        &mut variants,
        pattern_lower.replace('_', "-"),
        &pattern_lower,
    );

    // If the pattern has _ or - separators, also push a flat (no-separator)
    // concatenation so snake_case/kebab-case queries can match camelCase code lines.
    if pattern_lower.contains('_') || pattern_lower.contains('-') {
        let flat = pattern_lower
            .chars()
            .filter(|ch| *ch != '_' && *ch != '-')
            .collect::<String>();
        if !flat.is_empty() {
            push_unique_variant(&mut variants, flat, &pattern_lower);
        }
    }

    // CamelCase boundary: split into components, yield snake_case, kebab-case,
    // and flat-lowercase variants.
    if has_camel_case_boundary(pattern) {
        let components = split_camel_case_components(pattern);
        if components.len() > 1 {
            let snake = components.join("_");
            push_unique_variant(&mut variants, snake.clone(), &pattern_lower);
            let kebab = components.join("-");
            push_unique_variant(&mut variants, kebab, &pattern_lower);
            let flat = components.concat();
            push_unique_variant(&mut variants, flat, &pattern_lower);
        }
    }

    // Existing punctuation-escape branch: feed its variants through lowercasing.
    let unescaped = strip_punctuation_escapes(pattern);
    if unescaped != pattern {
        let unescaped_lower = unescaped.to_lowercase();
        push_unique_variant(&mut variants, unescaped_lower.clone(), &pattern_lower);
        push_unique_variant(
            &mut variants,
            unescaped_lower.replace('-', "_"),
            &pattern_lower,
        );
        push_unique_variant(
            &mut variants,
            unescaped_lower.replace('_', "-"),
            &pattern_lower,
        );
    }

    variants
}

fn push_unique_variant(variants: &mut Vec<String>, candidate: String, original: &str) {
    if candidate != original && !variants.contains(&candidate) {
        variants.push(candidate);
    }
}

fn strip_punctuation_escapes(pattern: &str) -> String {
    let mut stripped = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\'
            && let Some(next) = chars.peek().copied()
            && is_escaped_punctuation(next)
        {
            stripped.push(next);
            chars.next();
            continue;
        }
        stripped.push(ch);
    }

    stripped
}

fn is_escaped_punctuation(ch: char) -> bool {
    ch.is_ascii_punctuation() && ch != '\\'
}

pub(crate) fn tokenize_text_for_line_match(text: &str) -> HashSet<String> {
    let mut tokenizer = CodeTokenizer::with_default_patterns();
    let mut stream = tokenizer.token_stream(text);
    let mut tokens = HashSet::new();
    while stream.advance() {
        tokens.insert(stream.token().text.clone());
    }
    tokens
}

fn tokenize_text_sequence(text: &str) -> Vec<String> {
    let mut tokenizer = CodeTokenizer::with_default_patterns();
    let mut stream = tokenizer.token_stream(text);
    let mut tokens = Vec::new();
    while stream.advance() {
        tokens.push(stream.token().text.clone());
    }
    tokens
}

fn strip_balanced_wrapping_quotes(pattern: &str) -> Option<&str> {
    let mut chars = pattern.chars();
    let first = chars.next()?;
    if first != '"' && first != '\'' {
        return None;
    }
    if !pattern.ends_with(first) || pattern.len() < first.len_utf8() * 2 {
        return None;
    }

    Some(&pattern[first.len_utf8()..pattern.len() - first.len_utf8()])
}

fn line_matches_tokenized_phrase(line: &str, phrase: &str) -> bool {
    let phrase_tokens = tokenize_text_sequence(phrase);
    if phrase_tokens.is_empty() {
        return false;
    }

    let line_tokens = tokenize_text_sequence(line);
    if line_tokens.len() < phrase_tokens.len() {
        return false;
    }

    line_tokens
        .windows(phrase_tokens.len())
        .any(|window| window == phrase_tokens.as_slice())
}

fn line_matches_punctuation_normalized_phrase(line: &str, phrase: &str) -> bool {
    let phrase_tokens = tokenize_punctuation_normalized_sequence(phrase);
    if phrase_tokens.is_empty() {
        return false;
    }

    let line_tokens = tokenize_punctuation_normalized_sequence(line);
    if line_tokens.len() < phrase_tokens.len() {
        return false;
    }

    token_sequence_contains_contiguous_window(&line_tokens, &phrase_tokens)
}

fn tokenize_punctuation_normalized_sequence(text: &str) -> Vec<String> {
    let normalized = text
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_punctuation() { ' ' } else { ch })
        .collect::<String>();

    tokenize_text_sequence(&normalized)
}

fn token_sequence_contains_contiguous_window(haystack: &[String], needle: &[String]) -> bool {
    if needle.is_empty() {
        return false;
    }

    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

pub(crate) fn term_matches_line(term: &str, line: &str, line_tokens: &HashSet<String>) -> bool {
    if is_compound_term(term) {
        return line_matches_literal(line, term);
    }

    term_matches_tokens(term, line_tokens)
}

fn is_compound_term(term: &str) -> bool {
    (term.chars().any(|ch| ch.is_ascii_punctuation()) || has_camel_case_boundary(term))
        && tokenize_text_sequence(term).len() > 1
}

fn has_camel_case_boundary(term: &str) -> bool {
    let mut previous_was_lower_or_digit = false;

    for ch in term.chars() {
        if previous_was_lower_or_digit && ch.is_uppercase() {
            return true;
        }
        previous_was_lower_or_digit = ch.is_lowercase() || ch.is_ascii_digit();
    }

    false
}

fn split_camel_case_components(term: &str) -> Vec<String> {
    let mut components = Vec::new();
    let mut current = String::new();

    for ch in term.chars() {
        if ch.is_uppercase() && !current.is_empty() {
            components.push(std::mem::take(&mut current));
        }
        current.push(ch.to_ascii_lowercase());
    }
    if !current.is_empty() {
        components.push(current);
    }

    components
}

fn term_matches_tokens(term: &str, line_tokens: &HashSet<String>) -> bool {
    let mut tokenizer = CodeTokenizer::with_default_patterns();
    let mut stream = tokenizer.token_stream(term);
    while stream.advance() {
        if line_tokens.contains(&stream.token().text) {
            return true;
        }
    }
    false
}
