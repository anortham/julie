//! Memory checkpoint embedding: format, embed, and hybrid search.
//!
//! Provides the bridge between the memory system (Tantivy BM25) and the
//! embedding system (sqlite-vec KNN). Three layers:
//!
//! 1. **Format**: `format_checkpoint_for_embedding` — deterministic text representation
//! 2. **Embed**: `embed_checkpoint` / `embed_checkpoints_batch` — embed + store in DB
//! 3. **Search**: `hybrid_memory_search` — BM25 + KNN merged via weighted RRF

use anyhow::Result;
use tracing::{debug, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::search::weights::SearchWeightProfile;

use super::index::MemoryIndex;
use super::{Checkpoint, MemorySearchResult};

/// Format a checkpoint into a single text string for embedding.
///
/// Parallels the code embedding format (`"{kind} {name} {signature} {doc_comment}"`).
/// Includes the most semantically relevant fields — tags, decision, impact, symbols,
/// and the description (truncated to 512 chars to stay within embedding model limits).
pub fn format_checkpoint_for_embedding(checkpoint: &Checkpoint) -> String {
    let mut parts = Vec::new();

    // Tags first — they're the high-level category signal
    if let Some(ref tags) = checkpoint.tags {
        if !tags.is_empty() {
            parts.push(tags.join(" "));
        }
    }

    // Decision — the most semantically dense field
    if let Some(ref decision) = checkpoint.decision {
        if !decision.is_empty() {
            parts.push(decision.clone());
        }
    }

    // Impact — why this matters
    if let Some(ref impact) = checkpoint.impact {
        if !impact.is_empty() {
            parts.push(impact.clone());
        }
    }

    // Symbols — code references for cross-content alignment
    if let Some(ref symbols) = checkpoint.symbols {
        if !symbols.is_empty() {
            parts.push(symbols.join(" "));
        }
    }

    // Description — truncated to leave room for structured fields
    let desc = if checkpoint.description.len() > 512 {
        &checkpoint.description[..512]
    } else {
        &checkpoint.description
    };
    parts.push(desc.to_string());

    parts.join(" ")
}

/// Embed a single checkpoint and store its vector in `memory_vectors`.
pub fn embed_checkpoint(
    checkpoint: &Checkpoint,
    db: &mut SymbolDatabase,
    provider: &dyn EmbeddingProvider,
) -> Result<()> {
    let text = format_checkpoint_for_embedding(checkpoint);
    let vector = provider.embed_query(&text)?;
    db.store_memory_embeddings(&[(checkpoint.id.clone(), vector)])?;
    Ok(())
}

/// Embed a batch of checkpoints and store their vectors.
/// Returns the number of embeddings stored.
pub fn embed_checkpoints_batch(
    checkpoints: &[Checkpoint],
    db: &mut SymbolDatabase,
    provider: &dyn EmbeddingProvider,
) -> Result<usize> {
    if checkpoints.is_empty() {
        return Ok(0);
    }

    let texts: Vec<String> = checkpoints
        .iter()
        .map(format_checkpoint_for_embedding)
        .collect();

    let vectors = provider.embed_batch(&texts)?;

    let embeddings: Vec<(String, Vec<f32>)> = checkpoints
        .iter()
        .zip(vectors)
        .map(|(cp, vec)| (cp.id.clone(), vec))
        .collect();

    db.store_memory_embeddings(&embeddings)
}

/// Run hybrid memory search: BM25 (Tantivy) + KNN (sqlite-vec), merged via weighted RRF.
///
/// Graceful degradation:
/// - If `provider` is `None`, returns BM25 results only.
/// - If embedding or KNN fails, logs the error and falls back to BM25-only.
pub fn hybrid_memory_search(
    query: &str,
    memory_index: &MemoryIndex,
    db: &SymbolDatabase,
    provider: Option<&dyn EmbeddingProvider>,
    limit: usize,
) -> Result<Vec<MemorySearchResult>> {
    let bm25_limit = if provider.is_some() {
        limit * 2
    } else {
        limit
    };

    // Step 1: BM25 keyword search (always runs)
    let bm25_results = memory_index.search(query, bm25_limit)?;

    // Step 2: If no provider, return BM25 results directly
    let provider = match provider {
        Some(p) => p,
        None => {
            let mut results = bm25_results;
            results.truncate(limit);
            return Ok(results);
        }
    };

    // Step 3: Try semantic search — any failure degrades gracefully
    let knn_results = match run_memory_semantic_search(query, limit * 2, db, provider) {
        Ok(results) => results,
        Err(e) => {
            warn!("Memory semantic search failed, falling back to BM25-only: {e}");
            let mut results = bm25_results;
            results.truncate(limit);
            return Ok(results);
        }
    };

    // Step 4: Merge via weighted RRF using recall profile (memory-heavy)
    let profile = SearchWeightProfile::recall();

    debug!(
        "Memory hybrid merge: {} BM25 + {} semantic → weighted RRF (limit {})",
        bm25_results.len(),
        knn_results.len(),
        limit
    );

    let merged = weighted_memory_rrf_merge(
        bm25_results,
        knn_results,
        60,
        limit,
        profile.keyword_weight,
        profile.semantic_weight,
    );

    Ok(merged)
}

/// Internal: run the memory semantic search pipeline (embed query → KNN → convert).
fn run_memory_semantic_search(
    query: &str,
    limit: usize,
    db: &SymbolDatabase,
    provider: &dyn EmbeddingProvider,
) -> Result<Vec<MemorySearchResult>> {
    let query_vector = provider.embed_query(query)?;
    let knn_hits = db.knn_memory_search(&query_vector, limit)?;

    // Convert KNN results to MemorySearchResult with score = 1.0 - distance
    let results = knn_hits
        .into_iter()
        .map(|(checkpoint_id, distance)| MemorySearchResult {
            id: checkpoint_id,
            score: (1.0 - distance) as f32,
            // KNN doesn't carry full fields — these get filled in by the caller
            // when loading from disk (same pattern as recall_search_mode)
            body: String::new(),
            tags: String::new(),
            symbols: String::new(),
            decision: String::new(),
            impact: String::new(),
            branch: String::new(),
            timestamp: String::new(),
            file_path: String::new(),
        })
        .collect();

    Ok(results)
}

/// Weighted RRF merge for MemorySearchResult (mirrors hybrid.rs pattern).
///
/// Merges two ranked lists of memory search results using weighted Reciprocal
/// Rank Fusion. Uses `id` for deduplication.
fn weighted_memory_rrf_merge(
    bm25_results: Vec<MemorySearchResult>,
    knn_results: Vec<MemorySearchResult>,
    k: u32,
    limit: usize,
    keyword_weight: f32,
    semantic_weight: f32,
) -> Vec<MemorySearchResult> {
    use std::collections::HashMap;

    if knn_results.is_empty() {
        let mut results = bm25_results;
        results.truncate(limit);
        return results;
    }
    if bm25_results.is_empty() {
        let mut results = knn_results;
        results.truncate(limit);
        return results;
    }

    let k_f32 = k as f32;

    let mut scores: HashMap<String, f32> = HashMap::new();
    let mut results_by_id: HashMap<String, MemorySearchResult> = HashMap::new();

    for (i, result) in bm25_results.into_iter().enumerate() {
        let rank = (i + 1) as f32;
        let rrf_score = keyword_weight * (1.0 / (k_f32 + rank));
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
        results_by_id.entry(result.id.clone()).or_insert(result);
    }

    for (i, result) in knn_results.into_iter().enumerate() {
        let rank = (i + 1) as f32;
        let rrf_score = semantic_weight * (1.0 / (k_f32 + rank));
        *scores.entry(result.id.clone()).or_insert(0.0) += rrf_score;
        // BM25 results have richer fields — prefer them over KNN stubs
        results_by_id.entry(result.id.clone()).or_insert(result);
    }

    let mut merged: Vec<MemorySearchResult> = results_by_id
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
