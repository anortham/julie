//! Hybrid search: Reciprocal Rank Fusion (RRF) merge.
//!
//! Merges keyword (Tantivy) and semantic (KNN) ranked lists into a single
//! unified ranking using RRF. Documents appearing in both lists naturally
//! rank highest because they accumulate scores from both sources.
//!
//! Formula: `RRF(d) = Σ 1/(k + rank)` where rank is 1-based position.

use std::collections::HashMap;

use super::SymbolSearchResult;

/// Merge two ranked lists of search results using Reciprocal Rank Fusion.
///
/// # Arguments
/// - `tantivy_results`: Keyword search results (ordered by Tantivy score)
/// - `semantic_results`: Semantic/embedding search results (ordered by similarity)
/// - `k`: RRF smoothing constant (typically 60). Higher values reduce the
///   influence of high-ranking items relative to lower-ranking ones.
/// - `limit`: Maximum number of results to return
///
/// # Returns
/// Merged results sorted by RRF score descending. Each result's `score` field
/// is replaced with its RRF score.
pub fn rrf_merge(
    tantivy_results: Vec<SymbolSearchResult>,
    semantic_results: Vec<SymbolSearchResult>,
    k: u32,
    limit: usize,
) -> Vec<SymbolSearchResult> {
    // Fast path: if one list is empty, return the other (capped at limit)
    if semantic_results.is_empty() {
        let mut results = tantivy_results;
        results.truncate(limit);
        return results;
    }
    if tantivy_results.is_empty() {
        let mut results = semantic_results;
        results.truncate(limit);
        return results;
    }

    let k_f32 = k as f32;

    // Accumulate RRF scores keyed by symbol id
    let mut scores: HashMap<String, f32> = HashMap::new();
    // Keep one representative SymbolSearchResult per id (first seen wins)
    let mut results_by_id: HashMap<String, SymbolSearchResult> = HashMap::new();

    // Score tantivy results (1-based rank)
    for (i, result) in tantivy_results.into_iter().enumerate() {
        let rank = (i + 1) as f32;
        let rrf_score = 1.0 / (k_f32 + rank);
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
        results_by_id.entry(result.id.clone()).or_insert(result);
    }

    // Score semantic results (1-based rank)
    for (i, result) in semantic_results.into_iter().enumerate() {
        let rank = (i + 1) as f32;
        let rrf_score = 1.0 / (k_f32 + rank);
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
        results_by_id.entry(result.id.clone()).or_insert(result);
    }

    // Collect, sort by RRF score descending, and truncate
    let mut merged: Vec<SymbolSearchResult> = results_by_id
        .into_values()
        .map(|mut result| {
            result.score = scores[&result.id];
            result
        })
        .collect();

    merged.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    merged.truncate(limit);

    merged
}
