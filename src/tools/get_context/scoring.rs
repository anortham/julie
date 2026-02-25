//! Pivot selection and scoring for get_context.
//!
//! Combines text relevance, centrality boost, and context penalties (test file de-boost,
//! non-code file de-boost, structural kind de-boost, import filtering) to select the
//! best pivot symbols from search results.

use std::collections::HashMap;

use crate::search::index::SymbolSearchResult;
use crate::search::scoring::{CENTRALITY_WEIGHT, CENTRALITY_NOISE_NAMES};

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
    let results: Vec<_> = results
        .into_iter()
        .filter(|r| r.kind != "import")
        .collect();

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

    // Sort by combined score descending
    scored.sort_by(|a, b| {
        b.combined_score
            .partial_cmp(&a.combined_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

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
fn is_structural_kind(kind: &str) -> bool {
    matches!(kind, "namespace" | "module" | "export")
}

/// Check if a file path is a non-code file (docs, memories, markdown, config).
/// These files match on keywords but don't provide code orientation context.
fn is_non_code_path(path: &str) -> bool {
    path.starts_with(".memories/")
        || path.starts_with("docs/")
        || path.ends_with(".md")
        || path.ends_with(".toml")
        || path.ends_with(".json")
        || path.ends_with(".yaml")
        || path.ends_with(".yml")
}
