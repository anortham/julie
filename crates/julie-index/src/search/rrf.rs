use std::collections::HashMap;

use super::SymbolSearchResult;

/// Merge two ranked lists of search results using Reciprocal Rank Fusion.
/// Formula: `RRF(d) = Σ 1/(k + rank)`.
pub fn rrf_merge(
    tantivy_results: Vec<SymbolSearchResult>,
    semantic_results: Vec<SymbolSearchResult>,
    k: u32,
    limit: usize,
) -> Vec<SymbolSearchResult> {
    weighted_rrf_merge(tantivy_results, semantic_results, k, limit, 1.0, 1.0)
}

/// Weighted variant of `rrf_merge` — applies per-source weights to the RRF formula.
///
/// Formula: `score(d) = keyword_weight * 1/(k + rank_keyword(d)) + semantic_weight * 1/(k + rank_semantic(d))`
///
/// When both weights are 1.0, this produces identical results to `rrf_merge`.
pub fn weighted_rrf_merge(
    tantivy_results: Vec<SymbolSearchResult>,
    semantic_results: Vec<SymbolSearchResult>,
    k: u32,
    limit: usize,
    keyword_weight: f32,
    semantic_weight: f32,
) -> Vec<SymbolSearchResult> {
    // Fast path: if one list is empty, return the other (weighted but still RRF-scored)
    if semantic_results.is_empty() {
        let mut results = tantivy_results;
        let k_f32 = k as f32;
        for (i, result) in results.iter_mut().enumerate() {
            result.score = keyword_weight * (1.0 / (k_f32 + (i + 1) as f32));
        }
        results.truncate(limit);
        return results;
    }
    if tantivy_results.is_empty() {
        let mut results = semantic_results;
        let k_f32 = k as f32;
        for (i, result) in results.iter_mut().enumerate() {
            result.score = semantic_weight * (1.0 / (k_f32 + (i + 1) as f32));
        }
        results.truncate(limit);
        return results;
    }

    let k_f32 = k as f32;

    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut results_by_id: HashMap<String, SymbolSearchResult> = HashMap::new();

    for (i, result) in tantivy_results.into_iter().enumerate() {
        let rank = (i + 1) as f32;
        let rrf_score = keyword_weight * (1.0 / (k_f32 + rank));
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
        results_by_id.entry(result.id.clone()).or_insert(result);
    }

    for (i, result) in semantic_results.into_iter().enumerate() {
        let rank = (i + 1) as f32;
        let rrf_score = semantic_weight * (1.0 / (k_f32 + rank));
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
        // Overwrite keyword metadata: semantic results come from SQLite (source of truth),
        // while keyword results use Tantivy stored fields which may be stale.
        results_by_id.insert(result.id.clone(), result);
    }

    let mut merged: Vec<SymbolSearchResult> = results_by_id
        .into_values()
        .map(|mut result| {
            result.score = scores[&result.id];
            result
        })
        .collect();

    merged.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    merged.truncate(limit);

    merged
}
