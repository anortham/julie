//! Semantic similarity over a snapshot's vector set, shared by `deep_dive`
//! (similar symbols) and `fast_refs` (zero-reference fallback).

use julie_core::Symbol;

use crate::graph::SymbolId;
use crate::snapshot::Snapshot;

/// Minimum cosine similarity between two symbol vectors to count as related.
pub const MIN_SIMILARITY_SCORE: f32 = 0.5;

/// Minimum cosine similarity between a query vector and a symbol vector. Lower
/// than the symbol-to-symbol threshold because a raw name is compared against
/// rich metadata text.
pub const QUERY_SIMILARITY_THRESHOLD: f32 = 0.2;

/// A semantically similar symbol with its cosine score (higher is closer).
#[derive(Debug)]
pub struct SimilarEntry {
    pub symbol: Symbol,
    pub score: f32,
}

/// Symbols whose vectors are closest to `id`'s own, excluding `id` and
/// anything below `min_score`. Empty when `id` has no vector.
pub fn similar_to_symbol(
    snapshot: &Snapshot,
    id: SymbolId,
    limit: usize,
    min_score: f32,
) -> Vec<(SymbolId, f32)> {
    let vectors = snapshot.vectors();
    let Some(own) = vectors.vector_of(id) else {
        return Vec::new();
    };
    vectors
        .scan(own, limit + 1)
        .into_iter()
        .filter(|(hit, score)| *hit != id && *score >= min_score)
        .take(limit)
        .collect()
}

/// Symbols whose vectors are closest to `query`, at or above `min_score`.
pub fn similar_to_query(
    snapshot: &Snapshot,
    query: &[f32],
    limit: usize,
    min_score: f32,
) -> Vec<(SymbolId, f32)> {
    snapshot
        .vectors()
        .scan(query, limit)
        .into_iter()
        .filter(|(_, score)| *score >= min_score)
        .collect()
}
