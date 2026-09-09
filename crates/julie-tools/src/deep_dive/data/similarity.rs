use super::types::SimilarEntry;
use anyhow::Result;
use julie_core::Symbol;
use julie_core::database::SymbolDatabase;

/// Find semantically similar symbols via KNN on stored embeddings.
/// Delegates to the shared similarity module.
pub(crate) fn build_similar(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<SimilarEntry>> {
    if db.get_latest_ready_generation()?.is_none() {
        return Ok(Vec::new());
    }
    use julie_index::search::similarity::{self, MIN_SIMILARITY_SCORE};
    const SIMILAR_LIMIT: usize = 5;
    similarity::find_similar_symbols(db, symbol, SIMILAR_LIMIT, MIN_SIMILARITY_SCORE)
}
