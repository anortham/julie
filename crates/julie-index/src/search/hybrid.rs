//! Hybrid search: Tantivy keyword search over a snapshot's searcher merged
//! with vector hits from the snapshot's vector set via Reciprocal Rank Fusion.
//!
//! Key functions:
//! - `compute_query_embedding_for_hybrid` / `compute_tagged_query_embedding_for_hybrid`:
//!   compute the query embedding **before** the search, so a slow sidecar
//!   round-trip (up to 30 s) never sits on the search path.
//! - `vector_search`: the vector side alone, a cosine scan over the snapshot's
//!   vector set, shaped as symbol results.
//! - `hybrid_search_with_tagged_embedding`: keyword search plus the vector
//!   side, merged with weighted RRF. Degrades to keyword-only in `Auto` mode
//!   and fails closed in `Required` mode when vectors are not ready.
//! - `rrf_merge`: `RRF(d) = Σ 1/(k + rank)`.

use anyhow::Result;
use tracing::warn;

use super::SymbolSearchResult;
use super::index::{SearchFilter, SearchView, SymbolSearchResults};
use super::scoring::{classify_role, test_subrole};
use super::weights::SearchWeightProfile;
use crate::graph::{Graph, SymbolId};
use crate::snapshot::Snapshot;
use julie_core::embeddings_contract::{
    EmbeddingProvider, EmbeddingRequestBudget, SemanticMode, TaggedQueryEmbedding,
};

/// Per-side over-fetch multiplier (4×) for hybrid candidate pool aligning with keyword path.
const HYBRID_CANDIDATE_OVERFETCH_FACTOR: usize = 4;

pub use super::rrf::{rrf_merge, weighted_rrf_merge};

/// Compute the query embedding before the search runs.
pub fn compute_query_embedding_for_hybrid(
    query: &str,
    provider: Option<&dyn EmbeddingProvider>,
    budget: &EmbeddingRequestBudget,
) -> Option<Vec<f32>> {
    provider?.embed_query(query, budget).ok()
}

/// Compute the query embedding tagged with the provider's encoder key.
pub fn compute_tagged_query_embedding_for_hybrid(
    query: &str,
    provider: Option<&dyn EmbeddingProvider>,
    budget: &EmbeddingRequestBudget,
    semantic_mode: SemanticMode,
) -> Result<Option<TaggedQueryEmbedding>> {
    budget.check_budget()?;

    let Some(provider) = provider else {
        if semantic_mode == SemanticMode::Required {
            anyhow::bail!("SEMANTICS_NOT_READY: Embedding provider unavailable");
        }
        return Ok(None);
    };

    let encoder_key = match provider.encoder_identity().and_then(|id| id.storage_key()) {
        Ok(key) => key,
        Err(e) => {
            if semantic_mode == SemanticMode::Required {
                return Err(e.context("SEMANTICS_NOT_READY: Encoder identity unavailable"));
            }
            return Ok(None);
        }
    };

    let vector = match provider.embed_query(query, budget) {
        Ok(v) => v,
        Err(e) => {
            budget.check_budget()?;
            if semantic_mode == SemanticMode::Required {
                return Err(e.context("SEMANTICS_NOT_READY: embed_query failed"));
            }
            return Ok(None);
        }
    };

    budget.check_budget()?;

    Ok(Some(TaggedQueryEmbedding {
        vector,
        encoder_key,
        source_revision: 0,
    }))
}

/// A search result for the graph symbol `id`, scored `score`, shaped like
/// a Tantivy hit so RRF merges and filters treat both sides alike.
pub fn symbol_search_result(graph: &Graph, id: SymbolId, score: f32) -> SymbolSearchResult {
    let row = graph.symbol(id);
    let meta = row.metadata.as_ref();
    let is_test = meta
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let test_role = meta
        .and_then(|m| m.get("test_role"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| test_subrole(&row.path).to_string());
    let role = if is_test {
        "test"
    } else {
        classify_role(&row.path, &row.language)
    };
    SymbolSearchResult {
        id: row.id.clone(),
        name: row.name.clone(),
        signature: row.signature.clone().unwrap_or_default(),
        doc_comment: row.doc_comment.clone().unwrap_or_default(),
        file_path: row.path.clone(),
        kind: row.kind.to_string(),
        language: row.language.clone(),
        start_line: row.span.start_line,
        score,
        role: role.to_string(),
        test_role,
    }
}

/// Vector hits for `tagged` from the snapshot's vector set, best first. A set
/// with no vectors, or one built by another encoder, is the not-ready outcome:
/// `Required` fails closed, every other mode yields no semantic hits.
pub fn vector_search(
    snapshot: &Snapshot,
    tagged: &TaggedQueryEmbedding,
    limit: usize,
    semantic_mode: SemanticMode,
) -> Result<Vec<SymbolSearchResult>> {
    let vectors = snapshot.vectors();
    let ready = vectors
        .encoder()
        .is_some_and(|encoder| encoder.id == tagged.encoder_key)
        && !vectors.is_empty();
    if !ready {
        if semantic_mode == SemanticMode::Required {
            anyhow::bail!(
                "SEMANTICS_NOT_READY: no vectors for encoder '{}' in this snapshot",
                tagged.encoder_key
            );
        }
        return Ok(Vec::new());
    }
    if tagged.vector.len() != vectors.dims() {
        anyhow::bail!(
            "query vector has {} dimensions, the snapshot's vectors have {}",
            tagged.vector.len(),
            vectors.dims()
        );
    }
    let graph = snapshot.graph();
    Ok(vectors
        .scan(&tagged.vector, limit)
        .into_iter()
        .map(|(id, score)| symbol_search_result(graph, id, score.max(0.0)))
        .collect())
}

/// Hybrid search over `snapshot` using a tagged query embedding with explicit semantic mode.
pub fn hybrid_search_with_tagged_embedding(
    snapshot: &Snapshot,
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    query_embedding: Option<TaggedQueryEmbedding>,
    weight_profile: Option<SearchWeightProfile>,
    semantic_mode: SemanticMode,
) -> Result<SymbolSearchResults> {
    let tantivy_limit = if query_embedding.is_some() && semantic_mode != SemanticMode::Off {
        limit.saturating_mul(HYBRID_CANDIDATE_OVERFETCH_FACTOR)
    } else {
        limit
    };
    let tantivy_results = SearchView::new(snapshot.searcher(), snapshot.fields()).search_symbols(
        query,
        filter,
        tantivy_limit,
    )?;

    if semantic_mode == SemanticMode::Off {
        return Ok(tantivy_results);
    }

    let tagged = match query_embedding {
        Some(t) => t,
        None => {
            if semantic_mode == SemanticMode::Required {
                anyhow::bail!("SEMANTICS_NOT_READY: Query embedding required but not provided");
            }
            return Ok(tantivy_results);
        }
    };

    let semantic_results = match vector_search(snapshot, &tagged, tantivy_limit, semantic_mode) {
        Ok(results) => results,
        Err(e) => {
            if semantic_mode == SemanticMode::Required {
                return Err(e);
            }
            warn!("hybrid search: KNN search failed, degrading to keyword-only: {e}");
            Vec::new()
        }
    };

    let semantic_results: Vec<SymbolSearchResult> = semantic_results
        .into_iter()
        .filter(|r| filter.matches_symbol_result(r))
        .collect();

    let profile = match weight_profile {
        Some(p) => p,
        None => crate::search::weights::classify_query(query).to_weight_profile(),
    };
    let merged = weighted_rrf_merge(
        tantivy_results.results,
        semantic_results,
        60,
        limit,
        profile.keyword_weight,
        profile.semantic_weight,
    );

    Ok(SymbolSearchResults {
        results: merged,
        relaxed: tantivy_results.relaxed,
    })
}
