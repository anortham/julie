//! Pivot selection and scoring for get_context.
//!
//! Combines text relevance, centrality boost, and context penalties (test file de-boost,
//! non-code file de-boost, structural kind de-boost, import filtering) to select the
//! best pivot symbols from search results.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::search::index::SymbolSearchResult;
use crate::search::scoring::{CENTRALITY_NOISE_NAMES, CENTRALITY_WEIGHT};

/// A pivot symbol selected from search results, with its combined score.
pub struct Pivot {
    pub result: SymbolSearchResult,
    pub combined_score: f32,
}

/// Score penalty applied to results from test files.
/// Test functions match well on text (names contain search terms) but rarely
/// provide the context users actually want.
const TEST_FILE_PENALTY: f32 = 0.3;

/// Score penalty for non-code files (docs, memories, markdown).
/// get_context is for code orientation — documentation is useful but secondary.
const NON_CODE_PENALTY: f32 = 0.15;

/// Score penalty for auxiliary code paths (examples/benches/scripts).
/// These files can be useful, but production `src/` code is usually a better pivot.
const AUXILIARY_CODE_PENALTY: f32 = 0.55;

/// Score penalty for structural declarations (namespace, module, export).
/// These are 1-line boilerplate (`pub mod foo;`, `namespace Bar {}`) that match
/// on keywords but carry no useful code context.
const STRUCTURAL_KIND_PENALTY: f32 = 0.2;

/// Select pivot symbols from search results using centrality-weighted scoring.
///
/// Applies centrality boost to each result's text relevance score, then selects
/// an adaptive number of pivots based on score distribution:
/// - Top result 2x+ above second -> 1 pivot (clear winner)
/// - Top 3 within 30% of each other -> 3 pivots (cluster)
/// - Otherwise -> 2 pivots (default)
pub fn select_pivots(
    results: Vec<SymbolSearchResult>,
    reference_scores: &HashMap<String, f64>,
) -> Vec<Pivot> {
    if results.is_empty() {
        return Vec::new();
    }

    // Filter out import nodes — they never contain useful code context
    let results: Vec<_> = results.into_iter().filter(|r| r.kind != "import").collect();

    if results.is_empty() {
        return Vec::new();
    }

    // Compute combined scores: text_relevance * centrality_boost * context_penalty
    let mut scored: Vec<Pivot> = results
        .into_iter()
        .map(|r| {
            let ref_score = reference_scores.get(&r.id).copied().unwrap_or(0.0);
            let boost = if ref_score > 0.0 && !CENTRALITY_NOISE_NAMES.contains(&r.name.as_str()) {
                1.0 + (1.0 + ref_score as f32).ln() * CENTRALITY_WEIGHT
            } else {
                1.0
            };

            // De-boost non-code, test files, and structural declarations.
            // Factors are multiplicative so a namespace in a test file gets both penalties.
            let path_factor = if is_non_code_path(&r.file_path) {
                NON_CODE_PENALTY
            } else if is_test_path(&r.file_path) {
                TEST_FILE_PENALTY
            } else if is_auxiliary_path(&r.file_path) {
                AUXILIARY_CODE_PENALTY
            } else {
                1.0
            };
            let kind_factor = if is_structural_kind(&r.kind) {
                STRUCTURAL_KIND_PENALTY
            } else {
                1.0
            };
            let context_factor = path_factor * kind_factor;

            let combined = r.score * boost * context_factor;
            Pivot {
                result: r,
                combined_score: combined,
            }
        })
        .collect();

    // Sort by combined score descending with deterministic tie-breakers.
    // This keeps output stable across runs when scores are equal/near-equal.
    scored.sort_by(compare_pivots);

    // Determine pivot count from score distribution
    let top_score = scored[0].combined_score;
    let pivot_count = if scored.len() == 1 {
        1
    } else if top_score > scored[1].combined_score * 2.0 {
        1 // Clear winner — top result dominates
    } else if scored.len() >= 3 && scored[2].combined_score >= top_score * 0.7 {
        3 // Cluster — top 3 are close
    } else {
        2 // Default — top 2
    };

    scored.into_iter().take(pivot_count).collect()
}

/// Select pivots with a code-first fallback pass.
///
/// Primary pass uses standard centrality-weighted scoring. If the selected pivots
/// are entirely non-actionable (docs/memories/tests/structural), run a second pass
/// over code candidates only.
///
/// Avoids cloning the full results vec in the common case (mostly code). Only
/// clones the code-candidate subset when a mixed corpus makes fallback possible.
pub fn select_pivots_with_code_fallback(
    results: Vec<SymbolSearchResult>,
    reference_scores: &HashMap<String, f64>,
) -> Vec<Pivot> {
    if results.is_empty() {
        return Vec::new();
    }

    let code_count = results.iter().filter(|r| is_code_candidate(r)).count();

    // Fast path: if all results are code candidates or too few alternatives exist,
    // standard selection is sufficient — no fallback possible, skip the clone.
    if code_count >= results.len() || code_count < 2 {
        return select_pivots(results, reference_scores);
    }

    // Mixed corpus: clone only the code subset for potential fallback
    let code_only: Vec<SymbolSearchResult> = results
        .iter()
        .filter(|r| is_code_candidate(r))
        .cloned()
        .collect();

    let primary = select_pivots(results, reference_scores);
    if primary.is_empty() {
        return primary;
    }

    let actionable_count = primary
        .iter()
        .filter(|p| is_code_candidate(&p.result))
        .count();

    // Fallback conditions:
    // 1) No actionable pivots were selected (hard fail)
    // 2) Actionable coverage is too low (<50%) and there are enough code alternatives
    let should_fallback =
        actionable_count == 0 || (actionable_count * 2 < primary.len() && code_only.len() >= 2);

    if should_fallback {
        select_pivots(code_only, reference_scores)
    } else {
        primary
    }
}

/// Check if a file path is a test file (test code rarely provides useful context).
pub(crate) fn is_test_path(path: &str) -> bool {
    path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("_test.")
        || path.contains("_tests.")
        || path.contains(".test.")
        || path.contains(".spec.")
}

/// Check if a symbol kind is a structural declaration (namespace, module, export).
/// These are boilerplate lines that match on keywords but carry no useful code body.
pub(crate) fn is_structural_kind(kind: &str) -> bool {
    matches!(kind, "namespace" | "module" | "export")
}

/// Check if a file path is a non-code file (docs, memories, markdown, config).
/// These files match on keywords but don't provide code orientation context.
pub(crate) fn is_non_code_path(path: &str) -> bool {
    path.starts_with(".memories/")
        || path.starts_with("docs/")
        || path.ends_with(".md")
        || path.ends_with(".toml")
        || path.ends_with(".json")
        || path.ends_with(".yaml")
        || path.ends_with(".yml")
}

fn is_auxiliary_path(path: &str) -> bool {
    path.starts_with("examples/")
        || path.starts_with("example/")
        || path.starts_with("benches/")
        || path.starts_with("bench/")
        || path.starts_with("scripts/")
        || path.starts_with("fixtures/")
        || path.starts_with("fixture/")
        || path.starts_with("testdata/")
        || path.starts_with("test-data/")
        || path.starts_with("test_data/")
}

fn is_code_candidate(result: &SymbolSearchResult) -> bool {
    result.kind != "import"
        && !is_test_path(&result.file_path)
        && !is_non_code_path(&result.file_path)
        && !is_structural_kind(&result.kind)
}

fn compare_pivots(a: &Pivot, b: &Pivot) -> Ordering {
    // Primary: higher combined score first
    let score_order = b
        .combined_score
        .partial_cmp(&a.combined_score)
        .unwrap_or(Ordering::Equal);
    if score_order != Ordering::Equal {
        return score_order;
    }

    // Tie-break 1: prefer actionable code candidate
    let a_code = is_code_candidate(&a.result);
    let b_code = is_code_candidate(&b.result);
    if a_code != b_code {
        return b_code.cmp(&a_code);
    }

    // Tie-break 2: lexical stability
    a.result
        .name
        .cmp(&b.result.name)
        .then_with(|| a.result.file_path.cmp(&b.result.file_path))
        .then_with(|| a.result.id.cmp(&b.result.id))
}
