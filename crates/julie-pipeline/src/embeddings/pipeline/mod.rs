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
use julie_index::graph::{Graph, SymbolId};
use julie_index::search::language_config::LanguageConfigs;
use julie_index::vectors::encoder_row;
use tracing::info;

use batch::process_embedding_batches;
use enrichment::{build_callee_map, build_field_access_map, build_implementor_map};
use symbols::graph_symbols;

use crate::embeddings::EmbeddingProvider;
use crate::embeddings::metadata::{
    GLOBAL_VARIABLE_EMBEDDING_CAP, VariableEmbeddingPolicy, format_symbol_metadata,
    is_embeddable_for_language, is_embeddable_language, is_test_symbol_for_embedding,
    prepare_batch_for_embedding, select_budgeted_variable_ids,
};

/// Texts per sidecar request. The sidecar splits them into smaller GPU
/// batches; a smaller request keeps each round trip inside its deadline.
const EMBEDDING_BATCH_SIZE: usize = 50;
const VARIABLE_EMBEDDING_POLICY: VariableEmbeddingPolicy = VariableEmbeddingPolicy {
    enabled: true,
    max_ratio: 0.20,
};

fn canonical_symbols(graph: &Graph) -> Vec<(SymbolId, julie_core::Symbol)> {
    let graph_symbols = graph_symbols(graph);
    let mut order = Vec::new();
    let mut candidates: HashMap<SymbolId, Vec<julie_core::Symbol>> = HashMap::new();
    for (_, symbol) in &graph_symbols {
        let Some(id) = graph.symbol_by_row_id(&symbol.id) else {
            continue;
        };
        if !candidates.contains_key(&id) {
            order.push(id);
        }
        candidates.entry(id).or_default().push(symbol.clone());
    }
    order
        .into_iter()
        .filter_map(|id| {
            let choices = candidates.remove(&id)?;
            let symbol = choices
                .iter()
                .find(|symbol| !is_test_symbol_for_embedding(symbol))
                .cloned()
                .unwrap_or_else(|| choices[0].clone());
            Some((id, symbol))
        })
        .collect()
}

fn select_eligible_symbols(
    graph: &Graph,
    lang_configs: Option<&LanguageConfigs>,
) -> Vec<(SymbolId, julie_core::Symbol)> {
    let canonical = canonical_symbols(graph);
    let symbols: Vec<_> = canonical.iter().map(|(_, symbol)| symbol.clone()).collect();
    let base_ids: HashSet<String> = symbols
        .iter()
        .filter(|symbol| {
            is_embeddable_for_language(&symbol.kind, &symbol.language, lang_configs)
                && is_embeddable_language(&symbol.language)
                && !is_test_symbol_for_embedding(symbol)
        })
        .map(|symbol| symbol.id.clone())
        .collect();
    let scores: HashMap<_, _> = canonical
        .iter()
        .filter(|(_, symbol)| symbol.kind == SymbolKind::Variable)
        .map(|(id, symbol)| (symbol.id.clone(), graph.reference_score(*id)))
        .collect();
    let variable_ids: HashSet<_> = select_budgeted_variable_ids(
        &symbols,
        &scores,
        base_ids.len(),
        &VARIABLE_EMBEDDING_POLICY,
        lang_configs,
    )
    .into_iter()
    .collect();
    canonical
        .into_iter()
        .filter_map(|(id, symbol)| {
            (base_ids.contains(&symbol.id) || variable_ids.contains(&symbol.id))
                .then_some((id, symbol))
        })
        .collect()
}

pub fn select_eligible_symbol_ids(
    graph: &Graph,
    lang_configs: Option<&LanguageConfigs>,
) -> Vec<SymbolId> {
    select_eligible_symbols(graph, lang_configs)
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

pub fn eligible_vector_coverage(
    snapshot: &julie_index::snapshot::Snapshot,
    lang_configs: Option<&LanguageConfigs>,
) -> (usize, usize) {
    let eligible = select_eligible_symbol_ids(snapshot.graph(), lang_configs);
    let embedded = eligible
        .iter()
        .filter(|id| snapshot.vectors().contains(**id))
        .count();
    (eligible.len(), embedded)
}

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
    let canonical = canonical_symbols(graph);
    let eligible_ids: HashSet<_> = select_eligible_symbol_ids(graph, lang_configs)
        .into_iter()
        .collect();
    let eligible_row_ids: HashSet<_> = eligible_ids
        .iter()
        .map(|id| graph.symbol(*id).id.clone())
        .collect();
    let graph_symbols: Vec<_> = canonical
        .iter()
        .filter(|(id, _)| eligible_ids.contains(id))
        .cloned()
        .collect();
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

    let callees_by_symbol = build_callee_map(graph, &canonical);
    let fields_by_symbol = build_field_access_map(graph);
    let implementors_by_symbol = build_implementor_map(graph, &canonical);
    let all_symbols: Vec<_> = canonical.iter().map(|(_, symbol)| symbol.clone()).collect();
    let symbols: Vec<_> = graph_symbols
        .iter()
        .map(|(_, symbol)| symbol.clone())
        .collect();

    let base_prepared = prepare_batch_for_embedding(
        &all_symbols,
        lang_configs,
        &callees_by_symbol,
        &fields_by_symbol,
        &implementors_by_symbol,
    )
    .into_iter()
    .filter(|(id, _)| eligible_row_ids.contains(id))
    .collect::<Vec<_>>();
    let selected_variables: Vec<_> = symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Variable)
        .map(|symbol| (symbol.id.clone(), format_symbol_metadata(symbol)))
        .collect();
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
