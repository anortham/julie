//! Post-search scoring and reranking.
//!
//! Applies language-specific score boosts based on important_patterns
//! from language configurations. Results whose signatures match patterns
//! like "pub fn", "public class" etc. get a 1.5x score multiplier.

use std::collections::HashMap;

use crate::search::index::SymbolSearchResult;
use crate::search::language_config::LanguageConfigs;

/// Score multiplier for results matching an important pattern.
const IMPORTANT_PATTERN_BOOST: f32 = 1.5;

/// Weight for graph centrality boost (logarithmic scaling).
pub const CENTRALITY_WEIGHT: f32 = 0.3;

/// Conservative path prior multipliers for natural-language queries only.
///
/// The intent is to gently prefer production code over docs/tests/fixtures when
/// the query looks like natural language, without overwhelming text relevance.
pub(crate) const NL_PATH_BOOST_SRC: f32 = 1.08;
pub(crate) const NL_PATH_PENALTY_DOCS: f32 = 0.95;
pub(crate) const NL_PATH_PENALTY_TESTS: f32 = 0.95;
pub(crate) const NL_PATH_PENALTY_FIXTURES: f32 = 0.75;

/// Symbol names that are too ubiquitous to benefit from centrality scoring.
///
/// These are standard trait impls and common short names that accumulate
/// thousands of references across any codebase. Without filtering, `to_string`
/// (3702 refs) or `clone` (1665 refs) would get massive centrality boosts that
/// warp search rankings. Their high ref counts reflect language mechanics, not
/// actual importance.
///
/// NOTE: Intentionally separate from `NOISE_NEIGHBOR_NAMES` in get_context pipeline,
/// which serves a different purpose (neighbor expansion filtering) and has a different
/// membership set.
pub(crate) const CENTRALITY_NOISE_NAMES: &[&str] = &[
    "clone",
    "to_string",
    "fmt",
    "eq",
    "ne",
    "cmp",
    "partial_cmp",
    "hash",
    "drop",
    "deref",
    "deref_mut",
    "new",
    "default",
    "from",
    "into",
    "is_empty",
    "len",
    "as_ref",
    "as_mut",
    "borrow",
    "borrow_mut",
];

/// Apply important_patterns boost to search results, then re-sort by score.
///
/// For each result, if its signature contains any important_pattern from
/// the result's language config, its score is multiplied by `IMPORTANT_PATTERN_BOOST`.
/// Only one boost is applied per result regardless of how many patterns match.
///
/// After boosting, results are re-sorted by score descending.
pub fn apply_important_patterns_boost(
    results: &mut Vec<SymbolSearchResult>,
    configs: &LanguageConfigs,
) {
    for result in results.iter_mut() {
        if let Some(config) = configs.get(&result.language) {
            for pattern in &config.scoring.important_patterns {
                if result.signature.contains(pattern.as_str()) {
                    result.score *= IMPORTANT_PATTERN_BOOST;
                    break; // Only boost once per result
                }
            }
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Apply graph centrality boost to search results, then re-sort.
///
/// Symbols that are referenced more frequently across the codebase get a
/// logarithmic score boost. This promotes well-connected, "important"
/// symbols (e.g. core interfaces, heavily-used utilities) in search rankings.
///
/// Formula: `boosted = score * (1.0 + ln(1 + reference_score) * CENTRALITY_WEIGHT)`
///
/// The logarithmic scaling ensures diminishing returns — a symbol with 100
/// references doesn't dominate 10x more than one with 10 references.
pub fn apply_centrality_boost(
    results: &mut Vec<SymbolSearchResult>,
    reference_scores: &HashMap<String, f64>,
) {
    for result in results.iter_mut() {
        if CENTRALITY_NOISE_NAMES.contains(&result.name.as_str()) {
            continue; // Skip noise — ubiquitous trait impls shouldn't benefit from centrality
        }
        if let Some(&ref_score) = reference_scores.get(&result.id) {
            if ref_score > 0.0 {
                let boost = 1.0 + (1.0 + ref_score as f32).ln() * CENTRALITY_WEIGHT;
                result.score *= boost;
            }
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Apply a conservative path prior for natural-language-like queries.
///
/// For NL-like queries, this mildly boosts production source results and mildly
/// penalizes test, docs, and fixture paths. Uses language-agnostic heuristics
/// that work across Rust, C#, Python, Java, Go, JS/TS, Ruby, Swift, and more.
///
/// Identifier-like queries are explicitly excluded so exact symbol searches
/// are not perturbed.
pub fn apply_nl_path_prior(results: &mut [SymbolSearchResult], query: &str) {
    if !is_nl_like_query(query) {
        return;
    }

    for result in results.iter_mut() {
        let path = result.file_path.as_str();

        // Order matters: check test before source, since test paths may live
        // inside source directories (e.g. src/tests/, src/test/java/).
        if is_test_path(path) {
            result.score *= NL_PATH_PENALTY_TESTS;
        } else if is_docs_path(path) {
            result.score *= NL_PATH_PENALTY_DOCS;
        } else if is_fixture_path(path) {
            result.score *= NL_PATH_PENALTY_FIXTURES;
        } else {
            // Everything that isn't test/docs/fixtures is presumed source code.
            result.score *= NL_PATH_BOOST_SRC;
        }
    }

    sort_results_by_score_desc(results);
}

/// Detect whether a file path indicates test code, using language-agnostic heuristics.
///
/// Matches on both path segments (directories) and file-name conventions:
/// - Directories: `test`, `tests`, `spec`, `__tests__`, and `.Tests` (C#)
/// - Go files: `*_test.go`
/// - JS/TS files: `*.test.{js,ts,tsx,jsx}`, `*.spec.{js,ts,tsx,jsx}`
/// - Python files: `test_*.py`
pub(crate) fn is_test_path(path: &str) -> bool {
    // Check path segments (directory names)
    for segment in path.split('/') {
        // Exact segment matches
        match segment {
            "test" | "tests" | "Test" | "Tests" | "spec" | "Spec" | "__tests__" => return true,
            _ => {}
        }
        // C# convention: MyProject.Tests
        if segment.ends_with(".Tests") || segment.ends_with(".Test") {
            return true;
        }
    }

    // Check file-name patterns for languages that co-locate tests with source
    let file_name = path.rsplit('/').next().unwrap_or(path);

    // Go: auth_test.go
    if file_name.ends_with("_test.go") {
        return true;
    }

    // C/C++: jq_test.c, parser_test.cpp
    if file_name.ends_with("_test.c")
        || file_name.ends_with("_test.cc")
        || file_name.ends_with("_test.cpp")
    {
        return true;
    }

    // JS/TS: Auth.test.tsx, Auth.spec.ts, etc.
    let test_spec_extensions = [
        ".test.ts",
        ".test.tsx",
        ".test.js",
        ".test.jsx",
        ".spec.ts",
        ".spec.tsx",
        ".spec.js",
        ".spec.jsx",
    ];
    for ext in &test_spec_extensions {
        if file_name.ends_with(ext) {
            return true;
        }
    }

    // Python: test_auth.py (file starts with test_)
    if file_name.starts_with("test_") && file_name.ends_with(".py") {
        return true;
    }

    false
}

/// Detect whether a file path indicates documentation.
///
/// Matches path segments: `docs`, `doc`, `documentation`.
pub(crate) fn is_docs_path(path: &str) -> bool {
    for segment in path.split('/') {
        match segment {
            "docs" | "doc" | "documentation" | "Docs" | "Doc" | "Documentation" => return true,
            _ => {}
        }
    }
    false
}

/// Detect whether a file path indicates test fixtures or data.
///
/// Matches path segments: `fixtures`, `fixture`, `testdata`, `test_data`,
/// `test-data`, `__fixtures__`, `snapshots`, `__snapshots__`, `benchmarks`,
/// `benchmark`.
/// Also matches title-case variants (`Fixtures`, `Fixture`, `Snapshots`,
/// `Benchmarks`, `Benchmark`).
pub(crate) fn is_fixture_path(path: &str) -> bool {
    for segment in path.split('/') {
        match segment {
            "fixtures" | "fixture" | "Fixtures" | "Fixture" | "testdata" | "test_data"
            | "test-data" | "__fixtures__" | "snapshots" | "Snapshots" | "__snapshots__"
            | "benchmarks" | "Benchmarks" | "benchmark" | "Benchmark" => {
                return true;
            }
            _ => {}
        }
    }
    false
}

pub(crate) fn is_nl_like_query(query: &str) -> bool {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.len() < 2 {
        return false;
    }

    if terms.iter().any(|term| looks_like_identifier_token(term)) {
        return false;
    }

    terms
        .iter()
        .any(|term| term.chars().any(|c| c.is_ascii_alphabetic()))
}

fn looks_like_identifier_token(term: &str) -> bool {
    if term.contains('_') {
        return true;
    }

    let has_lower = term.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = term.chars().any(|c| c.is_ascii_uppercase());

    has_lower && has_upper
}

/// Kinds that represent actual definitions (not references/imports).
const DEFINITION_KINDS: &[&str] = &[
    "class",
    "struct",
    "interface",
    "trait",
    "enum",
    "function",
    "method",
    "constructor",
    "module",
    "namespace",
    "type",
    "constant",
    "delegate",
];

/// Documentation/markup languages whose symbols should rank below code definitions.
/// When a markdown heading and a Go struct both match "Command" as definitions,
/// the Go struct is almost certainly what the user wants.
pub(crate) const DOC_LANGUAGES: &[&str] = &["markdown", "json", "toml", "yaml"];

/// Promote exact name matches to the top of results using a three-tier stable partition.
///
/// When `search_target="definitions"`, the actual definition of a symbol may rank
/// low in Tantivy (mentioned once in its definition vs. many times in references).
/// This function applies a three-tier stable partition:
///
/// 1. **Tier 1**: Exact name match + definition kind (class, struct, function, etc.)
/// 2. **Tier 2**: Exact name match + non-definition kind (import, variable, etc.)
/// 3. **Tier 3**: Everything else (non-exact matches)
///
/// Name matching handles qualified names: searching "Router" matches "Phoenix.Router"
/// because the last component matches. Full-name matches are preferred over component matches.
///
/// Within each tier, original relative order is preserved (stable partition).
pub(crate) fn promote_exact_name_matches(results: &mut Vec<SymbolSearchResult>, query: &str) {
    if results.is_empty() {
        return;
    }

    let query_lower = query.trim().to_lowercase();

    // Three-tier stable partition: collect into three groups and recombine.
    let mut definitions = Vec::new();
    let mut other_exact = Vec::new();
    let mut rest = Vec::new();

    for result in results.drain(..) {
        if is_name_match(&result.name, &query_lower) {
            if DEFINITION_KINDS.contains(&result.kind.as_str()) {
                definitions.push(result);
            } else {
                other_exact.push(result);
            }
        } else {
            rest.push(result);
        }
    }

    // Within definitions tier, sort by composite key:
    //   1. Full-name match > component match (preserves qualified name priority)
    //   2. Source code > test code > doc language (demotes test doubles and markdown)
    //   3. Score descending (tie-breaker)
    definitions.sort_by(|a, b| {
        let is_full_match =
            |r: &SymbolSearchResult| -> bool { r.name.to_lowercase() == query_lower };
        let file_tier = |r: &SymbolSearchResult| -> u8 {
            if DOC_LANGUAGES.contains(&r.language.as_str()) {
                2 // doc language — lowest priority
            } else if is_test_path(&r.file_path) {
                1 // test file — middle priority
            } else {
                0 // source code — highest priority
            }
        };
        // Full matches first (false < true, so negate: !true=false < !false=true)
        let a_full = !is_full_match(a);
        let b_full = !is_full_match(b);
        a_full
            .cmp(&b_full)
            .then_with(|| file_tier(a).cmp(&file_tier(b)))
            .then_with(|| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    results.extend(definitions);
    results.extend(other_exact);
    results.extend(rest);
}

/// Check if a symbol name matches a query, supporting qualified names.
/// Matches if the full name matches OR the last component of a dot-qualified name matches.
/// Examples:
///   - "Router" matches "Router" (exact)
///   - "Router" matches "Phoenix.Router" (last component)
///   - "Phoenix.Router" matches "Phoenix.Router" (exact)
///   - "Router" does NOT match "RouterHelper" (not a component match)
fn is_name_match(symbol_name: &str, query_lower: &str) -> bool {
    let name_lower = symbol_name.to_lowercase();
    if name_lower == query_lower {
        return true;
    }
    // Check if query matches the last component of a qualified name (e.g. "Router" matches "Phoenix.Router")
    if let Some(last_component) = name_lower.rsplit('.').next() {
        if last_component == query_lower {
            return true;
        }
    }
    // Check if query is itself qualified and matches a suffix (e.g. "Channel.Server" matches "Phoenix.Channel.Server")
    if query_lower.contains('.') && name_lower.ends_with(query_lower) {
        // Ensure it's a component boundary (preceded by '.' or start of string)
        let prefix_len = name_lower.len() - query_lower.len();
        if prefix_len == 0 || name_lower.as_bytes()[prefix_len - 1] == b'.' {
            return true;
        }
    }
    false
}

fn sort_results_by_score_desc(results: &mut [SymbolSearchResult]) {
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
}
