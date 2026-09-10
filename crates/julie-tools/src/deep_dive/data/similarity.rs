use super::types::SimilarEntry;
use crate::snapshot_rows::to_symbol;
use julie_index::graph::SymbolId;
use julie_index::search::similarity::{MIN_SIMILARITY_SCORE, similar_to_symbol};
use julie_index::snapshot::Snapshot;

const SIMILAR_LIMIT: usize = 5;

/// Symbols whose vectors are closest to `id`'s, at or above the
/// symbol-to-symbol threshold. Empty when the snapshot has no vector for `id`.
pub(crate) fn build_similar(snapshot: &Snapshot, id: SymbolId) -> Vec<SimilarEntry> {
    let graph = snapshot.graph();
    similar_to_symbol(snapshot, id, SIMILAR_LIMIT, MIN_SIMILARITY_SCORE)
        .into_iter()
        .map(|(similar, score)| SimilarEntry {
            symbol: to_symbol(graph, similar),
            score,
        })
        .collect()
}
