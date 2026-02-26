//! Hybrid search: RRF merge and KNN conversion.
//!
//! Two key functions for hybrid search:
//! - `rrf_merge`: Merges keyword (Tantivy) and semantic (KNN) ranked lists using
//!   Reciprocal Rank Fusion. Formula: `RRF(d) = Σ 1/(k + rank)`.
//! - `knn_to_search_results`: Converts sqlite-vec KNN output `(symbol_id, distance)`
//!   into `SymbolSearchResult` objects by looking up symbol metadata from the database.

use std::collections::HashMap;

use anyhow::Result;

use super::SymbolSearchResult;
use crate::database::SymbolDatabase;

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

/// Convert KNN search results (symbol_id, distance) into `SymbolSearchResult` objects.
///
/// Batch-fetches symbol metadata from the database and maps each KNN result to a
/// `SymbolSearchResult`. Missing symbols (e.g., deleted during incremental updates)
/// are silently skipped. Results preserve KNN order (most similar first).
///
/// # Score conversion
/// `score = (1.0 - distance) as f32` — higher values indicate greater similarity.
pub fn knn_to_search_results(
    knn_results: &[(String, f64)],
    db: &SymbolDatabase,
) -> Result<Vec<SymbolSearchResult>> {
    if knn_results.is_empty() {
        return Ok(Vec::new());
    }

    // Batch-fetch all symbols by ID
    let ids: Vec<String> = knn_results.iter().map(|(id, _)| id.clone()).collect();
    let symbols = db.get_symbols_by_ids(&ids)?;

    // Build lookup map: symbol ID → Symbol
    let symbol_map: HashMap<&str, _> = symbols.iter().map(|s| (s.id.as_str(), s)).collect();

    // Convert in KNN order, skipping missing symbols
    let results = knn_results
        .iter()
        .filter_map(|(id, distance)| {
            let sym = symbol_map.get(id.as_str())?;
            Some(SymbolSearchResult {
                id: sym.id.clone(),
                name: sym.name.clone(),
                kind: sym.kind.to_string(),
                language: sym.language.clone(),
                file_path: sym.file_path.clone(),
                start_line: sym.start_line,
                signature: sym.signature.clone().unwrap_or_default(),
                doc_comment: sym.doc_comment.clone().unwrap_or_default(),
                score: (1.0 - distance) as f32,
            })
        })
        .collect();

    Ok(results)
}
