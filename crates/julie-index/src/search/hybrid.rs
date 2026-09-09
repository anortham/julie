//! Hybrid search: combines Tantivy keyword search with KNN semantic search.
//!
//! Key functions:
//! - `hybrid_search`: Orchestrator that runs both search backends and merges results.
//!   Gracefully degrades to keyword-only when no embedding provider is available or
//!   when semantic search fails.
//! - `compute_query_embedding_for_hybrid`: Compute the query embedding **before**
//!   hybrid search. Prefer this so a slow sidecar round-trip (up to 30 s) does not
//!   sit on the search / write path.
//! - `hybrid_search_with_embedding`: Variant that accepts a pre-computed embedding
//!   and does no sidecar I/O during the search merge.
//! - `rrf_merge`: Merges keyword (Tantivy) and semantic (KNN) ranked lists using
//!   Reciprocal Rank Fusion. Formula: `RRF(d) = Σ 1/(k + rank)`.
//! - `knn_to_search_results`: Converts sqlite-vec KNN output `(symbol_id, distance)`
//!   into `SymbolSearchResult` objects by looking up symbol metadata from the database.

use std::collections::HashMap;

use anyhow::Result;
use tracing::{debug, info, warn};

use super::SymbolSearchResult;
use super::index::{SearchFilter, SearchIndex, SymbolSearchResults, symbol_role_and_test_role};
use super::weights::SearchWeightProfile;
use julie_core::database::SymbolDatabase;
use julie_core::embeddings_contract::{
    EmbeddingProvider, EmbeddingRequestBudget, SemanticMode, TaggedQueryEmbedding,
};

/// Per-side over-fetch multiplier (4×) for hybrid candidate pool aligning with keyword path.
const HYBRID_CANDIDATE_OVERFETCH_FACTOR: usize = 4;

pub use super::rrf::{rrf_merge, weighted_rrf_merge};

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

    // Convert in KNN order, skipping missing symbols. Project role fields from
    // the same path + metadata policy used by Tantivy symbol documents.
    let results = knn_results
        .iter()
        .filter_map(|(id, distance)| {
            let sym = symbol_map.get(id.as_str())?;
            let (role, test_role) =
                symbol_role_and_test_role(&sym.file_path, &sym.language, sym.metadata.as_ref());
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
                role,
                test_role,
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
    // Over-fetch when we'll merge; exact limit when keyword-only.
    // See HYBRID_CANDIDATE_OVERFETCH_FACTOR for the rationale on 4×.
    let tantivy_limit = if embedding_provider.is_some() {
        limit.saturating_mul(HYBRID_CANDIDATE_OVERFETCH_FACTOR)
    } else {
        limit
    };

    // Step 1: Tantivy keyword search (always runs)
    let tantivy_results = search_index.search_symbols_via_unified(query, filter, tantivy_limit)?;

    // Step 2: If no provider or generation unready, return keyword results directly
    let provider = match embedding_provider {
        Some(p) => {
            let key = p.encoder_identity().ok().and_then(|i| i.storage_key().ok());
            let rev = db
                .get_latest_canonical_revision_number()
                .ok()
                .flatten()
                .unwrap_or(0);
            if !key.is_some_and(|k| db.embedding_generation_ready(&k, rev).unwrap_or(false)) {
                return Ok(tantivy_results);
            }
            p
        }
        None => return Ok(tantivy_results),
    };

    // Step 3: Try semantic search — any failure degrades gracefully
    let semantic_results = match run_semantic_search(
        query,
        limit.saturating_mul(HYBRID_CANDIDATE_OVERFETCH_FACTOR),
        db,
        provider,
        &EmbeddingRequestBudget::default(),
    ) {
        Ok(results) => results,
        Err(e) => {
            warn!("Semantic search failed, falling back to keyword-only: {e}");
            Vec::new()
        }
    };

    // Enforce caller filter constraints on semantic candidates before merge.
    let semantic_results: Vec<SymbolSearchResult> = semantic_results
        .into_iter()
        .filter(|result| filter.matches_symbol_result(result))
        .collect();

    // Step 4: Merge via RRF (k=60), optionally weighted
    info!(
        "Hybrid merge: {} keyword + {} semantic results → {} (limit {})",
        tantivy_results.results.len(),
        semantic_results.len(),
        if weight_profile.is_some() {
            "weighted RRF (explicit)"
        } else {
            "weighted RRF (classified)"
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

    let profile = match weight_profile {
        Some(profile) => {
            debug!(
                "  weight profile (explicit): keyword={:.2}, semantic={:.2}",
                profile.keyword_weight, profile.semantic_weight
            );
            profile
        }
        None => {
            let intent = crate::search::weights::classify_query(query);
            let profile = intent.to_weight_profile();
            debug!(
                "  weight profile (classified {:?}): keyword={:.2}, semantic={:.2}",
                intent, profile.keyword_weight, profile.semantic_weight
            );
            profile
        }
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

/// Internal: run semantic search pipeline (embed → KNN → convert).
fn run_semantic_search(
    query: &str,
    limit: usize,
    db: &SymbolDatabase,
    provider: &dyn EmbeddingProvider,
    budget: &EmbeddingRequestBudget,
) -> Result<Vec<SymbolSearchResult>> {
    let query_vector = provider.embed_query(query, budget)?;
    let key = provider
        .encoder_identity()
        .ok()
        .and_then(|i| i.storage_key().ok());
    db.with_read_transaction(|tx_db| {
        if let Some(ref k) = key {
            let rev = tx_db.get_latest_canonical_revision_number()?.unwrap_or(0);
            if !tx_db.embedding_generation_ready(k, rev)? {
                anyhow::bail!("generation not ready");
            }
        }
        let knn_hits = tx_db.knn_search(&query_vector, limit)?;
        knn_to_search_results(&knn_hits, tx_db)
    })
}

/// Compute query embedding before acquiring index lock.
pub fn compute_query_embedding_for_hybrid(
    query: &str,
    provider: Option<&dyn EmbeddingProvider>,
    budget: &EmbeddingRequestBudget,
) -> Option<Vec<f32>> {
    provider?.embed_query(query, budget).ok()
}

/// Compute tagged query embedding bound to active ready generation.
pub fn compute_tagged_query_embedding_for_hybrid(
    query: &str,
    provider: Option<&dyn EmbeddingProvider>,
    budget: &EmbeddingRequestBudget,
    db: &SymbolDatabase,
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

    let rev = db.get_latest_canonical_revision_number()?.unwrap_or(0);
    if !db.embedding_generation_ready(&encoder_key, rev)? {
        if semantic_mode == SemanticMode::Required {
            anyhow::bail!(
                "SEMANTICS_NOT_READY: generation for encoder '{}' at rev {} is not ready",
                encoder_key,
                rev
            );
        }
        return Ok(None);
    }

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
        source_revision: rev,
    }))
}

/// Hybrid search using a tagged query embedding with explicit semantic mode and transaction isolation.
pub fn hybrid_search_with_tagged_embedding(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    search_index: &SearchIndex,
    db: &SymbolDatabase,
    query_embedding: Option<TaggedQueryEmbedding>,
    weight_profile: Option<SearchWeightProfile>,
    semantic_mode: SemanticMode,
) -> Result<SymbolSearchResults> {
    let tantivy_limit = if query_embedding.is_some() && semantic_mode != SemanticMode::Off {
        limit.saturating_mul(HYBRID_CANDIDATE_OVERFETCH_FACTOR)
    } else {
        limit
    };
    let tantivy_results = search_index.search_symbols_via_unified(query, filter, tantivy_limit)?;

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
    let knn_limit = limit.saturating_mul(HYBRID_CANDIDATE_OVERFETCH_FACTOR);
    let semantic_results_res = db.with_read_transaction(|tx_db| {
        let current_rev = tx_db.get_latest_canonical_revision_number()?.unwrap_or(0);
        if tagged.source_revision != current_rev {
            if semantic_mode == SemanticMode::Required {
                anyhow::bail!(
                    "SEMANTICS_NOT_READY: query revision {} differs from current canonical revision {}",
                    tagged.source_revision,
                    current_rev
                );
            }
            return Ok(Vec::new());
        }
        if !tx_db.embedding_generation_ready(&tagged.encoder_key, tagged.source_revision)? {
            if semantic_mode == SemanticMode::Required {
                anyhow::bail!(
                    "SEMANTICS_NOT_READY: generation for encoder '{}' at rev {} is not ready",
                    tagged.encoder_key,
                    tagged.source_revision
                );
            }
            return Ok(Vec::new());
        }
        let hits = tx_db.knn_search(&tagged.vector, knn_limit)?;
        knn_to_search_results(&hits, tx_db)
    });

    let semantic_results = match semantic_results_res {
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

/// Hybrid search using a pre-computed query embedding vector (auto mode).
pub fn hybrid_search_with_embedding(
    query: &str,
    filter: &SearchFilter,
    limit: usize,
    search_index: &SearchIndex,
    db: &SymbolDatabase,
    query_embedding: Option<Vec<f32>>,
    weight_profile: Option<SearchWeightProfile>,
) -> Result<SymbolSearchResults> {
    let tagged = query_embedding.map(TaggedQueryEmbedding::untagged);
    hybrid_search_with_tagged_embedding(
        query,
        filter,
        limit,
        search_index,
        db,
        tagged,
        weight_profile,
        SemanticMode::Auto,
    )
}
