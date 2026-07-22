//! Post-search scoring and reranking.
//!
//! Applies language-specific score boosts based on important_patterns
//! from language configurations. Results whose signatures match patterns
//! like "pub fn", "public class" etc. get a 1.5x score multiplier.

use std::collections::HashMap;

use crate::search::index::SymbolSearchResult;
use crate::search::language_config::LanguageConfigs;
use julie_core::database::SymbolDatabase;

/// Score multiplier for results matching an important pattern.
const IMPORTANT_PATTERN_BOOST: f32 = 1.5;

/// Weight for graph centrality boost (logarithmic scaling).
pub const CENTRALITY_WEIGHT: f32 = 0.3;

/// Conservative path prior multipliers for natural-language queries only.
///
/// The intent is to gently prefer production code over docs/tests/fixtures when
/// the query looks like natural language, without overwhelming text relevance.
pub const NL_PATH_BOOST_SRC: f32 = 1.08;
pub(crate) const NL_PATH_PENALTY_DOCS: f32 = 0.92;
pub const NL_PATH_PENALTY_TESTS: f32 = 0.85;
pub(crate) const NL_PATH_PENALTY_FIXTURES: f32 = 0.70;

/// Soft penalty applied to candidates whose language is not the workspace's
/// dominant language when running natural-language queries. Prevents Python
/// fixtures from outranking Rust production code on Rust-dominant repos.
pub(crate) const NL_LANGUAGE_AFFINITY_PENALTY: f32 = 0.85;

/// Minimum share (0.0–1.0) of files in a single language required to treat
/// it as the workspace's dominant language. Below this, the language
/// affinity prior is a no-op (mixed-language repos don't get penalized).
pub(crate) const NL_LANGUAGE_DOMINANCE_THRESHOLD: f64 = 0.70;

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
pub const CENTRALITY_NOISE_NAMES: &[&str] = &[
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
///
/// **Test-intent override**: when the query has test intent (tokens like
/// `test`, `tests`, `spec`, `fixture`, `conftest`, or `test_*`/`*_test`
/// shapes), the test-path penalty AND the source-path boost are both
/// skipped — otherwise tests for the queried behavior get pushed below their
/// production counterparts even though they are exactly what the user asked
/// for. Docs and fixtures still get their penalty. Caught by eros benchmark
/// (julie scored 0/16 on test-intent lookups).
pub fn apply_nl_path_prior(results: &mut [SymbolSearchResult], query: &str) {
    if !is_nl_like_query(query) {
        return;
    }

    let test_intent = has_test_intent(query);

    for result in results.iter_mut() {
        let path = result.file_path.as_str();

        // Order matters: check test before source, since test paths may live
        // inside source directories (e.g. src/tests/, src/test/java/).
        if is_test_path(path) {
            if !test_intent {
                result.score *= NL_PATH_PENALTY_TESTS;
            }
            // test_intent: leave test-path scores untouched so they compete
            // on BM25 with source candidates — the query terms typically
            // appear verbatim in the test function name.
        } else if is_docs_path(path) {
            result.score *= NL_PATH_PENALTY_DOCS;
        } else if is_fixture_path(path) {
            result.score *= NL_PATH_PENALTY_FIXTURES;
        } else if !test_intent {
            // Everything that isn't test/docs/fixtures is presumed source code.
            // Skip the source boost on test-intent queries so we don't lift
            // production code above the tests that match the query verbatim.
            result.score *= NL_PATH_BOOST_SRC;
        }
    }

    sort_results_by_score_desc(results);
}

/// True when the query is asking about tests / specs / fixtures rather
/// than about production behavior.
///
/// Detects:
/// - Bare tokens: `test`, `tests`, `spec`, `specs`, `fixture`, `fixtures`,
///   `conftest`.
/// - Identifier shapes: `test_<thing>`, `<thing>_test`, `<thing>_spec`,
///   `spec_<thing>`.
///
/// Case-insensitive. Operates on whitespace-split tokens.
pub(crate) fn has_test_intent(query: &str) -> bool {
    const TEST_TOKENS: &[&str] = &[
        "test", "tests", "spec", "specs", "fixture", "fixtures", "conftest",
    ];

    for token in query.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if TEST_TOKENS.contains(&lower.as_str()) {
            return true;
        }
        if lower.starts_with("test_")
            || lower.starts_with("spec_")
            || lower.ends_with("_test")
            || lower.ends_with("_tests")
            || lower.ends_with("_spec")
            || lower.ends_with("_specs")
        {
            return true;
        }
    }

    false
}

/// Compute the workspace's dominant language if one language accounts for
/// at least [`NL_LANGUAGE_DOMINANCE_THRESHOLD`] of indexed files.
///
/// Returns `None` for mixed-language workspaces (no single language above
/// the threshold). The caller passes the result into
/// [`apply_language_affinity_prior`] — `None` makes that a no-op.
///
/// Cheap single SQL query (one row per language); call once per search.
pub fn compute_dominant_language(db: &SymbolDatabase) -> Option<String> {
    let counts = db.count_files_by_language().ok()?;
    if counts.is_empty() {
        return None;
    }
    let total: i64 = counts.iter().map(|(_, n)| *n).sum();
    if total <= 0 {
        return None;
    }
    let (lang, top_count) = counts.into_iter().next()?;
    if (top_count as f64) / (total as f64) >= NL_LANGUAGE_DOMINANCE_THRESHOLD {
        Some(lang)
    } else {
        None
    }
}

/// Demote candidates whose language differs from the workspace's dominant
/// language, for natural-language queries only.
///
/// Fixes the cross-language leakage observed in dogfood: Python test files
/// ranking #1 for Rust-targeted NL queries on a 95%-Rust workspace. The
/// penalty is soft (`NL_LANGUAGE_AFFINITY_PENALTY`) so a strongly-matched
/// foreign-language symbol can still surface.
///
/// No-op when:
/// - the workspace has no dominant language (mixed repo)
/// - the query looks like an identifier (exact symbol lookup)
pub fn apply_language_affinity_prior(
    results: &mut [SymbolSearchResult],
    dominant_language: Option<&str>,
    query: &str,
) {
    let Some(dominant) = dominant_language else {
        return;
    };
    if !is_nl_like_query(query) {
        return;
    }

    let mut touched = false;
    for result in results.iter_mut() {
        if result.language != dominant {
            result.score *= NL_LANGUAGE_AFFINITY_PENALTY;
            touched = true;
        }
    }

    if touched {
        sort_results_by_score_desc(results);
    }
}

/// Detect whether a file path indicates test code, using language-agnostic heuristics.
///
/// Matches on both path segments (directories) and file-name conventions:
/// - Directories: `test`, `tests`, `spec`, `__tests__`, and `.Tests` (C#)
/// - Go files: `*_test.go`
/// - JS/TS files: `*.test.{js,ts,tsx,jsx}`, `*.spec.{js,ts,tsx,jsx}`
/// - Python files: `test_*.py`
pub fn is_test_path(path: &str) -> bool {
    // Check path segments (directory names)
    for segment in path.split('/') {
        // Exact segment matches
        match segment {
            "test" | "tests" | "Test" | "Tests" | "spec" | "Spec" | "__tests__" => return true,
            _ => {}
        }
        // C# convention: MyProject.Tests, MyProject.Tests.Integration
        if segment.ends_with(".Tests")
            || segment.ends_with(".Test")
            || segment.contains(".Tests.")
            || segment.contains(".Test.")
        {
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
pub fn is_docs_path(path: &str) -> bool {
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
pub fn is_fixture_path(path: &str) -> bool {
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

/// Detect whether a file path indicates vendored / third-party code.
///
/// Matches path segments: `node_modules`, `vendor`, `third_party`, `deps`,
/// `external`, `Pods` (CocoaPods), `bower_components`.
pub(crate) fn is_vendor_path(path: &str) -> bool {
    for segment in path.split('/') {
        match segment {
            "node_modules" | "vendor" | "Vendor" | "third_party" | "third-party" | "deps"
            | "external" | "Pods" | "bower_components" => return true,
            _ => {}
        }
    }
    false
}

/// Detect whether a file path indicates generated / build-output code.
///
/// Matches path segments: `target`, `build`, `dist`, `out`, `bin`, `obj`,
/// `generated`, `__generated__`, `gen`.
pub(crate) fn is_generated_path(path: &str) -> bool {
    for segment in path.split('/') {
        match segment {
            "target" | "build" | "Build" | "dist" | "out" | "bin" | "obj" | "generated"
            | "Generated" | "__generated__" | "gen" => return true,
            _ => {}
        }
    }
    false
}

/// Classify a file path + language into a [`role`] string for the C.3
/// Tantivy schema. Ordering: vendor → generated → test → docs → source.
/// `test` is checked AFTER vendor/generated so that vendored tests don't
/// pollute the test bucket.
pub fn classify_role(path: &str, language: &str) -> &'static str {
    if is_vendor_path(path) {
        "vendor"
    } else if is_generated_path(path) {
        "generated"
    } else if is_test_path(path) {
        "test"
    } else if is_docs_path(path) || DOC_LANGUAGES.contains(&language) {
        "docs"
    } else {
        "source"
    }
}

/// If `path` is a test path, return its sub-role (`unit | integration |
/// smoke`) or empty string when no sub-role segment is present.
pub fn test_subrole(path: &str) -> &'static str {
    if !is_test_path(path) {
        return "";
    }
    for segment in path.split('/') {
        match segment {
            "integration" | "integration_tests" | "Integration" => return "integration",
            "smoke" | "Smoke" => return "smoke",
            "unit" | "Unit" => return "unit",
            _ => {}
        }
    }
    ""
}

/// True when `language` denotes source code (not docs/data formats).
pub(crate) fn is_source_language(language: &str) -> bool {
    !DOC_LANGUAGES.contains(&language)
}

pub fn is_nl_like_query(query: &str) -> bool {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.len() < 2 {
        return false;
    }

    // Veto only when EVERY term looks like an identifier — that's a pure
    // multi-symbol lookup (e.g. "parse_query score_candidate") which
    // should stay on the keyword-only path. Mixed queries that pair an
    // identifier with prose context (e.g. "how does fast_refs find callers",
    // "parse_query reranker intent classification") are NL-shaped and must
    // engage hybrid + reranker so docs don't outrank the actual definitions.
    if terms.iter().all(|term| looks_like_identifier_token(term)) {
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
pub(crate) const DEFINITION_KINDS: &[&str] = &[
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

/// Check if a symbol name matches a query, supporting qualified names.
/// Matches if the full name matches OR the last component of a dot-qualified name matches.
/// Examples:
///   - "Router" matches "Router" (exact)
///   - "Router" matches "Phoenix.Router" (last component)
///   - "Phoenix.Router" matches "Phoenix.Router" (exact)
///   - "Router" does NOT match "RouterHelper" (not a component match)
pub(crate) fn is_name_match(symbol_name: &str, query_lower: &str) -> bool {
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
