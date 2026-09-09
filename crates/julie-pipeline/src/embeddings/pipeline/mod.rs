//! Background embedding pipeline.
//!
//! After Tantivy indexing completes, this pipeline runs in the background to
//! generate vector embeddings for all embeddable symbols. It processes symbols
//! in batches to avoid holding the database lock for too long.

mod batch;
mod enrichment;
mod file_indexer;

#[cfg(test)]
mod tests;

pub use file_indexer::{embed_symbols_for_file, reembed_symbols_for_file};

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tracing::{info, warn};

use batch::process_embedding_batches;
use enrichment::{build_callee_map, build_field_access_map, build_implementor_map};

use crate::embeddings::EmbeddingProvider;
use crate::embeddings::metadata::{
    GLOBAL_VARIABLE_EMBEDDING_CAP, NON_EMBEDDABLE_LANGUAGES, VariableEmbeddingPolicy,
    prepare_batch_for_embedding, select_budgeted_variables,
};
use julie_core::database::SymbolDatabase;
use julie_extractors::SymbolKind;
use julie_index::search::language_config::LanguageConfigs;

/// Batch size for embedding generation (symbols per batch).
///
/// This controls how many texts are sent in a single JSON-RPC request to the
/// sidecar. The sidecar processes them internally in smaller GPU batches (default 32).
/// Smaller values here mean shorter per-request time and less risk of hitting the
/// response timeout — especially for larger models like CodeRankEmbed (768d).
const EMBEDDING_BATCH_SIZE: usize = 250;
const VARIABLE_EMBEDDING_POLICY: VariableEmbeddingPolicy = VariableEmbeddingPolicy {
    enabled: true,
    max_ratio: 0.20,
};

/// Embedding text format version. Bump when the format of text passed to the
/// embedding model changes (e.g., adding file paths, implementor names, field
/// signatures to symbol metadata). A version mismatch triggers a full re-embed
/// on the next pipeline run, so users upgrading Julie get enriched embeddings
/// automatically without needing `force: true`.
///
/// History:
///   1 = original format (symbol name + signature + children names)
///   2 = enriched format (+ file path, implementor names, field signatures)
///   3 = float16 inference (model weights loaded in half precision on GPU)
pub const EMBEDDING_FORMAT_VERSION: u32 = 3;

/// Statistics from an embedding pipeline run.
#[derive(Debug, Clone)]
pub struct EmbeddingStats {
    pub symbols_scanned: usize,
    pub symbols_embedded: usize,
    pub symbols_skipped: usize,
    pub batches_processed: usize,
}

/// Run the full embedding pipeline: load symbols → filter → embed → store.
///
/// This is designed to run in a `spawn_blocking` context since both the
/// embedding provider and database use synchronous Mutex access.
///
/// **Graceful degradation:** Returns `Err` on failure but never panics.
/// The caller should log errors and continue — keyword search is unaffected.
pub fn run_embedding_pipeline(
    db: &Arc<Mutex<SymbolDatabase>>,
    provider: &dyn EmbeddingProvider,
    lang_configs: Option<&LanguageConfigs>,
) -> Result<EmbeddingStats> {
    run_embedding_pipeline_cancellable(db, provider, lang_configs, None)
}

/// Reference variant for callers holding `&AtomicBool` instead of `Arc<AtomicBool>`.
pub fn run_embedding_pipeline_cancellable_ref(
    db: &Arc<Mutex<SymbolDatabase>>,
    provider: &dyn EmbeddingProvider,
    lang_configs: Option<&LanguageConfigs>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<EmbeddingStats> {
    let cancel_arc = cancel.map(|c| {
        Arc::new(std::sync::atomic::AtomicBool::new(
            c.load(std::sync::atomic::Ordering::Acquire),
        ))
    });
    run_embedding_pipeline_cancellable(db, provider, lang_configs, cancel_arc)
}

/// Cancellable variant. When `cancel` is set to `true`, the pipeline stops
/// after the current batch and returns what it has so far.
pub fn run_embedding_pipeline_cancellable(
    db: &Arc<Mutex<SymbolDatabase>>,
    provider: &dyn EmbeddingProvider,
    lang_configs: Option<&LanguageConfigs>,
    cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
) -> Result<EmbeddingStats> {
    let cancel_token =
        cancel.unwrap_or_else(|| Arc::new(std::sync::atomic::AtomicBool::new(false)));
    let mut stats = EmbeddingStats {
        symbols_scanned: 0,
        symbols_embedded: 0,
        symbols_skipped: 0,
        batches_processed: 0,
    };

    // Detect model/dimension/format changes and recreate the vector table if needed.
    // Any change wipes all vectors and re-embeds from scratch, because vectors from
    // different models or format versions are not comparable.
    let encoder_key = provider
        .encoder_identity()
        .and_then(|i| i.storage_key())
        .context("Provider encoder identity is unavailable; missing provenance prevents persistent generation creation")?;
    let provider_dims = provider.dimensions();

    let (generation_id, source_revision) = {
        let mut db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let rev = db_guard
            .get_latest_canonical_revision_number()?
            .unwrap_or(0);
        let gen_id = db_guard
            .begin_embedding_generation(&encoder_key, rev, provider_dims)
            .context("Failed to begin embedding generation")?;
        (gen_id, rev)
    };

    // Purge embeddings for non-code languages (markdown, json, toml, etc.)
    // before loading the incremental set, so purged symbols aren't in "already_embedded".
    {
        let mut db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let purged = db_guard
            .delete_embeddings_for_languages(NON_EMBEDDABLE_LANGUAGES)
            .context("Failed to purge non-code embeddings")?;
        if purged > 0 {
            info!("Embedding pipeline: purged {purged} non-code embeddings (markdown, json, etc.)");
        }
    }

    // Load all symbols, existing embedding IDs, and variable reference scores.
    let (symbols, already_embedded, variable_reference_scores) = {
        let db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let syms = db_guard
            .get_all_symbols()
            .context("Failed to load symbols for embedding")?;
        let embedded = db_guard
            .get_embedded_symbol_ids()
            .context("Failed to load existing embedding IDs")?;

        let variable_ids: Vec<&str> = syms
            .iter()
            .filter(|s| s.kind == SymbolKind::Variable)
            .map(|s| s.id.as_str())
            .collect();
        let reference_scores = match db_guard.get_reference_scores(&variable_ids) {
            Ok(scores) => scores,
            Err(err) => {
                warn!(
                    "Embedding pipeline: failed to load variable reference scores, using defaults: {err:#}"
                );
                HashMap::new()
            }
        };

        (syms, embedded, reference_scores)
    };

    stats.symbols_scanned = symbols.len();
    info!("Embedding pipeline: {} total symbols loaded", symbols.len());

    // Build callee map, field access map, and implementor map for enrichment.
    let (callees_by_symbol, fields_by_symbol, implementors_by_symbol) = {
        let db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        (
            build_callee_map(&db_guard, &symbols),
            build_field_access_map(&db_guard),
            build_implementor_map(&db_guard, &symbols),
        )
    };

    // Build base prepared symbols (existing embeddable kinds) and merge selected variables.
    let base_prepared = prepare_batch_for_embedding(
        &symbols,
        lang_configs,
        &callees_by_symbol,
        &fields_by_symbol,
        &implementors_by_symbol,
    );
    let candidate_variable_ids: HashSet<String> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| s.id.clone())
        .collect();
    let selected_variables = select_budgeted_variables(
        &symbols,
        &variable_reference_scores,
        base_prepared.len(),
        &VARIABLE_EMBEDDING_POLICY,
        lang_configs,
    );
    let selected_variable_ids: HashSet<String> = selected_variables
        .iter()
        .map(|(id, _)| id.clone())
        .collect();
    let per_language_cap =
        ((base_prepared.len() as f64) * VARIABLE_EMBEDDING_POLICY.max_ratio).floor() as usize;

    info!(
        "Embedding pipeline variable policy: candidates={}, per_language_cap={}, global_cap={}, selected={}",
        candidate_variable_ids.len(),
        per_language_cap,
        GLOBAL_VARIABLE_EMBEDDING_CAP,
        selected_variable_ids.len(),
    );

    let mut all_prepared = base_prepared;
    all_prepared.extend(selected_variables);
    if all_prepared.is_empty() {
        info!("Embedding pipeline: no embeddable symbols found, publishing empty ready generation");
        let mut db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let current_revision = db_guard
            .get_latest_canonical_revision_number()?
            .unwrap_or(0);
        if current_revision == source_revision {
            db_guard
                .publish_embedding_generation(generation_id, source_revision, 0, 0)
                .context("Failed to publish empty embedding generation")?;
        } else {
            let _ = db_guard.fail_embedding_generation(generation_id);
        }
        return Ok(stats);
    }

    // Purge stale embeddings: any previously-embedded symbol that is no longer
    // in the eligible set (e.g., test symbols after filter change, variables that
    // dropped below the budget cutoff, deleted symbols).
    let eligible_ids: HashSet<&str> = all_prepared.iter().map(|(id, _)| id.as_str()).collect();
    let stale_ids: Vec<String> = already_embedded
        .iter()
        .filter(|id| !eligible_ids.contains(id.as_str()))
        .cloned()
        .collect();
    let stale_deleted = if stale_ids.is_empty() {
        0
    } else {
        match db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?
            .delete_embeddings_for_symbol_ids(&stale_ids)
        {
            Ok(deleted) => deleted,
            Err(err) => {
                warn!("Embedding pipeline: failed to delete stale embeddings, continuing: {err:#}");
                0
            }
        }
    };
    if stale_deleted > 0 {
        info!("Embedding pipeline: purged {stale_deleted} stale embeddings");
    }

    // Symbols with enrichment data must be re-embedded when their enrichment
    // changes. Containers get child enrichment, functions/methods get callee
    // and field access enrichment.
    let enriched_ids: HashSet<&str> = symbols
        .iter()
        .filter(|s| match s.kind {
            SymbolKind::Class | SymbolKind::Struct | SymbolKind::Enum => true,
            SymbolKind::Interface | SymbolKind::Trait => true,
            SymbolKind::Function | SymbolKind::Method => {
                callees_by_symbol.contains_key(&s.id) || fields_by_symbol.contains_key(&s.id)
            }
            _ => false,
        })
        .map(|s| s.id.as_str())
        .collect();

    let total_eligible = all_prepared.len();
    // Skip symbols that already have embeddings (incremental),
    // EXCEPT enriched symbols which always get re-embedded.
    let prepared: Vec<_> = all_prepared
        .into_iter()
        .filter(|(id, _)| !already_embedded.contains(id) || enriched_ids.contains(id.as_str()))
        .collect();

    // Count skipped as symbols that were already embedded AND not being re-embedded
    let re_embedded_count = prepared
        .iter()
        .filter(|(id, _)| already_embedded.contains(id))
        .count();
    stats.symbols_skipped = already_embedded.len() - re_embedded_count;

    if prepared.is_empty() {
        info!(
            "Embedding pipeline: all {} embeddable symbols already embedded, publishing ready generation",
            stats.symbols_skipped
        );
        let mut db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let current_revision = db_guard
            .get_latest_canonical_revision_number()?
            .unwrap_or(0);
        if current_revision == source_revision {
            let total_embedded = db_guard.get_embedded_symbol_ids()?.len();
            db_guard
                .publish_embedding_generation(
                    generation_id,
                    source_revision,
                    total_eligible,
                    total_embedded,
                )
                .context("Failed to publish ready embedding generation")?;
        } else {
            let _ = db_guard.fail_embedding_generation(generation_id);
        }
        return Ok(stats);
    }

    info!(
        "Embedding pipeline: {} to embed ({} new, {} re-enriched, {} skipped, {} total)",
        prepared.len(),
        prepared.len() - re_embedded_count,
        re_embedded_count,
        stats.symbols_skipped,
        symbols.len()
    );

    let completed_cleanly = process_embedding_batches(
        db,
        provider,
        &prepared,
        generation_id,
        &cancel_token,
        &mut stats,
    )?;

    if completed_cleanly {
        let mut db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let current_revision = db_guard
            .get_latest_canonical_revision_number()?
            .unwrap_or(0);
        if current_revision == source_revision {
            let total_embedded = db_guard.get_embedded_symbol_ids()?.len();
            if total_embedded >= total_eligible {
                db_guard
                    .publish_embedding_generation(
                        generation_id,
                        source_revision,
                        total_eligible,
                        total_embedded,
                    )
                    .context("Failed to publish embedding generation")?;
                info!(
                    "Published embedding generation {generation_id} for revision {source_revision}: {}/{} symbols",
                    total_embedded, total_eligible
                );
            } else {
                warn!(
                    "Embedding generation {generation_id} incomplete: {total_embedded}/{total_eligible} symbols embedded; marking generation as failed"
                );
                let _ = db_guard.fail_embedding_generation(generation_id);
            }
        } else {
            warn!(
                "Source revision advanced from {source_revision} to {current_revision} during embedding; \
                 marking generation {generation_id} as failed"
            );
            let _ = db_guard.fail_embedding_generation(generation_id);
        }
    } else if let Ok(mut db_guard) = db.lock() {
        let _ = db_guard.fail_embedding_generation(generation_id);
    }

    info!(
        "Embedding pipeline complete: {}/{} symbols embedded in {} batches",
        stats.symbols_embedded, stats.symbols_scanned, stats.batches_processed
    );

    Ok(stats)
}
