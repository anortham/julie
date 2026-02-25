//! Pivot selection and scoring for get_context.
//!
//! Combines text relevance, centrality boost, and context penalties (test file de-boost,
//! import filtering) to select the best pivot symbols from search results.

use std::collections::HashMap;

use crate::search::index::SymbolSearchResult;
use crate::search::scoring::CENTRALITY_WEIGHT;

/// A pivot symbol selected from search results, with its combined score.
pub struct Pivot {
    pub result: SymbolSearchResult,
    pub combined_score: f32,
}

/// Score penalty applied to results from test files.
/// Test functions match well on text (names contain search terms) but rarely
/// provide the context users actually want.
const TEST_FILE_PENALTY: f32 = 0.3;

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
            let boost = if ref_score > 0.0 {
                1.0 + (1.0 + ref_score as f32).ln() * CENTRALITY_WEIGHT
            } else {
                1.0
            };

            // De-boost test files — test code rarely provides useful orientation context
            let test_factor = if is_test_path(&r.file_path) {
                TEST_FILE_PENALTY
            } else {
                1.0
            };

            let combined = r.score * boost * test_factor;
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
fn is_test_path(path: &str) -> bool {
    path.contains("/tests/")
        || path.contains("/test/")
        || path.contains("_test.")
        || path.contains("_tests.")
        || path.contains(".test.")
        || path.contains(".spec.")
}
