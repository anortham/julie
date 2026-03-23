//! Hybrid search: combines Tantivy keyword search with KNN semantic search.
//!
//! Three key functions:
//! - `hybrid_search`: Orchestrator that runs both search backends and merges results.
//!   Gracefully degrades to keyword-only when no embedding provider is available or
//!   when semantic search fails.
//! - `rrf_merge`: Merges keyword (Tantivy) and semantic (KNN) ranked lists using
//!   Reciprocal Rank Fusion. Formula: `RRF(d) = Σ 1/(k + rank)`.
//! - `knn_to_search_results`: Converts sqlite-vec KNN output `(symbol_id, distance)`
//!   into `SymbolSearchResult` objects by looking up symbol metadata from the database.

use std::collections::HashMap;

use anyhow::Result;
use tracing::{debug, info, warn};

use super::SymbolSearchResult;
use super::index::{SearchFilter, SearchIndex, SymbolSearchResults};
use super::weights::SearchWeightProfile;
use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::tools::search::query::matches_glob_pattern;

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
        results_by_id.entry(result.id.clone()).or_insert(result);
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
                score: (1.0 - distance).max(0.0) as f32,
            })
        })
        .collect();

    Ok(results)
}

/// Run hybrid search: Tantivy keyword + KNN semantic, merged via RRF.
///
/// When `weight_profile` is `Some`, uses `weighted_rrf_merge` with the profile's
/// keyword/semantic weights. When `None`, falls back to uniform `rrf_merge`.
///
/// Graceful degradation:
/// - If `embedding_provider` is `None`, returns Tantivy results directly (keyword-only).
/// - If embedding or KNN search fails, logs the error and falls back to keyword-only.
/// - The search NEVER fails due to embedding/KNN errors.
///
/// Over-fetches 2x from both sources when semantic search is active, giving RRF
/// a larger merge pool for better result quality.
pub fn hybrid_search(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    search_index: &SearchIndex,
    db: &SymbolDatabase,
    embedding_provider: Option<&dyn EmbeddingProvider>,
    weight_profile: Option<SearchWeightProfile>,
) -> Result<SymbolSearchResults> {
    // Over-fetch when we'll merge; exact limit when keyword-only
    let tantivy_limit = if embedding_provider.is_some() {
        limit * 2
    } else {
        limit
    };

    // Step 1: Tantivy keyword search (always runs)
    let tantivy_results = search_index.search_symbols(query, filter, tantivy_limit)?;

    // Step 2: If no embedding provider, return keyword results directly
    let provider = match embedding_provider {
        Some(p) => p,
        None => return Ok(tantivy_results),
    };

    // Step 3: Try semantic search — any failure degrades gracefully
    let semantic_results = match run_semantic_search(query, limit * 2, db, provider) {
        Ok(results) => results,
        Err(e) => {
            warn!("Semantic search failed, falling back to keyword-only: {e}");
            Vec::new()
        }
    };

    // Enforce caller filter constraints on semantic candidates before merge.
    let semantic_results: Vec<SymbolSearchResult> = semantic_results
        .into_iter()
        .filter(|result| matches_filter(result, filter))
        .collect();

    // Step 4: Merge via RRF (k=60), optionally weighted
    info!(
        "Hybrid merge: {} keyword + {} semantic results → {} (limit {})",
        tantivy_results.results.len(),
        semantic_results.len(),
        if weight_profile.is_some() {
            "weighted RRF"
        } else {
            "uniform RRF"
        },
        limit
    );

    // Trace top keyword and semantic results for diagnostics
    // Enable with RUST_LOG=julie::search::hybrid=debug
    if tracing::enabled!(tracing::Level::DEBUG) {
        let kw_top: Vec<_> = tantivy_results
            .results
            .iter()
            .take(10)
            .map(|r| format!("{}({:.3})", r.name, r.score))
            .collect();
        let sem_top: Vec<_> = semantic_results
            .iter()
            .take(10)
            .map(|r| format!("{}({:.3})", r.name, r.score))
            .collect();
        debug!("  keyword top-10: [{}]", kw_top.join(", "));
        debug!("  semantic top-10: [{}]", sem_top.join(", "));
    }

    let merged = match weight_profile {
        Some(profile) => {
            debug!(
                "  weight profile: keyword={:.2}, semantic={:.2}",
                profile.keyword_weight, profile.semantic_weight
            );
            weighted_rrf_merge(
                tantivy_results.results,
                semantic_results,
                60,
                limit,
                profile.keyword_weight,
                profile.semantic_weight,
            )
        }
        None => rrf_merge(tantivy_results.results, semantic_results, 60, limit),
    };

    Ok(SymbolSearchResults {
        results: merged,
        relaxed: tantivy_results.relaxed,
    })
}

fn matches_filter(result: &SymbolSearchResult, filter: &SearchFilter) -> bool {
    if let Some(language) = &filter.language {
        if result.language != *language {
            return false;
        }
    }

    if let Some(kind) = &filter.kind {
        if result.kind != *kind {
            return false;
        }
    }

    if let Some(file_pattern) = &filter.file_pattern {
        if !matches_glob_pattern(&result.file_path, file_pattern) {
            return false;
        }
    }

    true
}

/// Internal: run the semantic search pipeline (embed → KNN → convert).
///
/// Separated from `hybrid_search` so the orchestrator can catch errors from
/// any step in a single `match`.
fn run_semantic_search(
    query: &str,
    limit: usize,
    db: &SymbolDatabase,
    provider: &dyn EmbeddingProvider,
) -> Result<Vec<SymbolSearchResult>> {
    let query_vector = provider.embed_query(query)?;
    let knn_hits = db.knn_search(&query_vector, limit)?;
    knn_to_search_results(&knn_hits, db)
}
