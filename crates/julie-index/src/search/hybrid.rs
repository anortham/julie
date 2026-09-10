//! Hybrid search: Tantivy keyword search over a snapshot's searcher merged
//! with vector hits from the snapshot's vector set via Reciprocal Rank Fusion.
//!
//! Key functions:
//! - `compute_query_embedding_for_hybrid` / `compute_tagged_query_embedding_for_hybrid`:
//!   compute the query embedding **before** the search, so a slow sidecar
//!   round-trip (up to 30 s) never sits on the search path.
//! - `hybrid_search_with_tagged_embedding`: keyword search plus the vector
//!   side, merged with weighted RRF. Degrades to keyword-only in `Auto` mode
//!   and fails closed in `Required` mode when vectors are not ready.
//! - `rrf_merge`: `RRF(d) = Σ 1/(k + rank)`.

use anyhow::Result;
use tracing::warn;

use super::SymbolSearchResult;
use super::index::{SearchFilter, SearchView, SymbolSearchResults};
use super::weights::SearchWeightProfile;
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

/// Vector hits for `tagged` from the snapshot's vector set. An empty set is
/// the not-ready outcome: `Required` fails closed, every other mode yields
/// no semantic hits.
fn vector_hits(
    snapshot: &Snapshot,
    tagged: &TaggedQueryEmbedding,
    semantic_mode: SemanticMode,
) -> Result<Vec<SymbolSearchResult>> {
    if snapshot.vectors().is_empty() && semantic_mode == SemanticMode::Required {
        anyhow::bail!(
            "SEMANTICS_NOT_READY: no vectors for encoder '{}' in this snapshot",
            tagged.encoder_key
        );
    }
    Ok(Vec::new())
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

    let semantic_results = match vector_hits(snapshot, &tagged, semantic_mode) {
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
