//! Background embedding pipeline over a checkout store.
//!
//! Symbols of the current snapshot with no vector row for the active encoder
//! are formatted, embedded in batches, and written through
//! [`CheckoutStore::store_vectors`]; one publish at the end serves them.

mod batch;
mod enrichment;
mod symbols;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result};
use julie_extractors::SymbolKind;
use julie_index::checkout_store::CheckoutStore;
use julie_index::search::language_config::LanguageConfigs;
use julie_index::vectors::encoder_row;
use tracing::info;

use batch::process_embedding_batches;
use enrichment::{build_callee_map, build_field_access_map, build_implementor_map};
use symbols::graph_symbols;

use crate::embeddings::EmbeddingProvider;
use crate::embeddings::metadata::{
    GLOBAL_VARIABLE_EMBEDDING_CAP, VariableEmbeddingPolicy, prepare_batch_for_embedding,
    select_budgeted_variables,
};

/// Texts per sidecar request. The sidecar splits them into smaller GPU
/// batches; a smaller request keeps each round trip inside its deadline.
const EMBEDDING_BATCH_SIZE: usize = 50;
const VARIABLE_EMBEDDING_POLICY: VariableEmbeddingPolicy = VariableEmbeddingPolicy {
    enabled: true,
    max_ratio: 0.20,
};

/// Statistics from an embedding pipeline run.
#[derive(Debug, Clone)]
pub struct EmbeddingStats {
    pub symbols_scanned: usize,
    pub symbols_embedded: usize,
    pub symbols_skipped: usize,
    pub batches_processed: usize,
}

/// Run the pipeline to completion. Blocking; call from `spawn_blocking`.
pub fn run_embedding_pipeline(
    store: &CheckoutStore,
    provider: &dyn EmbeddingProvider,
    lang_configs: Option<&LanguageConfigs>,
) -> Result<EmbeddingStats> {
    run_embedding_pipeline_cancellable(store, provider, lang_configs, None)
}

/// Cancellable variant: when `cancel` becomes true the run stops after the
/// current batch and publishes what it stored. Returns `Err` only when the
/// encoder identity is unavailable or a store write fails; keyword search is
/// unaffected either way.
pub fn run_embedding_pipeline_cancellable(
    store: &CheckoutStore,
    provider: &dyn EmbeddingProvider,
    lang_configs: Option<&LanguageConfigs>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<EmbeddingStats> {
    let cancel_token = cancel.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    let mut stats = EmbeddingStats {
        symbols_scanned: 0,
        symbols_embedded: 0,
        symbols_skipped: 0,
        batches_processed: 0,
    };

    let identity = provider.encoder_identity().context(
        "Provider encoder identity is unavailable; missing provenance prevents storing vectors",
    )?;
    let encoder = encoder_row(&identity)?;
    if provider.dimensions() != encoder.dimensions as usize {
        anyhow::bail!(
            "Provider dimension mismatch: identity says {}, provider produces {}",
            encoder.dimensions,
            provider.dimensions()
        );
    }
    store.set_encoder(&encoder)?;

    let snapshot = store.current();
    let graph = snapshot.graph();
    let vectors = snapshot.vectors();
    let graph_symbols = graph_symbols(graph);
    stats.symbols_scanned = graph_symbols.len();
    info!(
        "Embedding pipeline: {} total symbols loaded",
        graph_symbols.len()
    );

    let already_embedded: HashSet<&str> = graph_symbols
        .iter()
        .filter(|(id, _)| vectors.contains(*id))
        .map(|(_, s)| s.id.as_str())
        .collect();
    let keys: HashMap<String, (String, u32)> = graph_symbols
        .iter()
        .map(|(id, s)| {
            let row = graph.symbol(*id);
            (s.id.clone(), (row.blob_hash.clone(), row.ordinal))
        })
        .collect();
    let variable_reference_scores: HashMap<String, f64> = graph_symbols
        .iter()
        .filter(|(_, s)| s.kind == SymbolKind::Variable)
        .map(|(id, s)| (s.id.clone(), graph.reference_score(*id)))
        .collect();

    let callees_by_symbol = build_callee_map(graph, &graph_symbols);
    let fields_by_symbol = build_field_access_map(graph);
    let implementors_by_symbol = build_implementor_map(graph, &graph_symbols);
    let symbols: Vec<_> = graph_symbols.iter().map(|(_, s)| s.clone()).collect();

    let base_prepared = prepare_batch_for_embedding(
        &symbols,
        lang_configs,
        &callees_by_symbol,
        &fields_by_symbol,
        &implementors_by_symbol,
    );
    let selected_variables = select_budgeted_variables(
        &symbols,
        &variable_reference_scores,
        base_prepared.len(),
        &VARIABLE_EMBEDDING_POLICY,
        lang_configs,
    );
    let per_language_cap =
        ((base_prepared.len() as f64) * VARIABLE_EMBEDDING_POLICY.max_ratio).floor() as usize;
    info!(
        "Embedding pipeline variable policy: candidates={}, per_language_cap={}, global_cap={}, selected={}",
        variable_reference_scores.len(),
        per_language_cap,
        GLOBAL_VARIABLE_EMBEDDING_CAP,
        selected_variables.len(),
    );

    let mut all_prepared = base_prepared;
    all_prepared.extend(selected_variables);
    let total_eligible = all_prepared.len();
    let prepared: Vec<_> = all_prepared
        .into_iter()
        .filter(|(id, _)| !already_embedded.contains(id.as_str()))
        .collect();
    stats.symbols_skipped = total_eligible - prepared.len();

    if prepared.is_empty() {
        info!(
            "Embedding pipeline: all {} embeddable symbols already embedded",
            stats.symbols_skipped
        );
        return Ok(stats);
    }
    info!(
        "Embedding pipeline: {} to embed ({} skipped, {} total)",
        prepared.len(),
        stats.symbols_skipped,
        symbols.len()
    );

    let completed = process_embedding_batches(
        store,
        provider,
        &prepared,
        &encoder.id,
        &keys,
        &cancel_token,
        &mut stats,
    );
    store.publish_vectors()?;
    completed?;

    info!(
        "Embedding pipeline complete: {}/{} symbols embedded in {} batches",
        stats.symbols_embedded, stats.symbols_scanned, stats.batches_processed
    );
    Ok(stats)
}
