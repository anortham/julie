//! Background embedding pipeline.
//!
//! After Tantivy indexing completes, this pipeline runs in the background to
//! generate vector embeddings for all embeddable symbols. It processes symbols
//! in batches to avoid holding the database lock for too long.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use tracing::{info, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::embeddings::metadata::{
    NON_EMBEDDABLE_LANGUAGES, VariableEmbeddingPolicy, prepare_batch_for_embedding,
    select_budgeted_variables,
};
use crate::extractors::SymbolKind;
use crate::search::language_config::LanguageConfigs;

/// Batch size for embedding generation (symbols per batch).
const EMBEDDING_BATCH_SIZE: usize = 500;
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
    let mut stats = EmbeddingStats {
        symbols_scanned: 0,
        symbols_embedded: 0,
        symbols_skipped: 0,
        batches_processed: 0,
    };

    // Detect model/dimension changes and recreate the vector table if needed.
    // Any model change (name or dimensions) wipes all vectors and re-embeds from scratch,
    // because vectors from different models are not comparable even at the same dimensionality.
    {
        let mut db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let (stored_model, stored_dims) = db_guard
            .get_embedding_config()
            .unwrap_or(("unknown".to_string(), 384));
        let provider_dims = provider.dimensions();
        let provider_model = provider.device_info().model_name;

        if stored_dims != provider_dims || stored_model != provider_model {
            info!(
                "Embedding pipeline: model change detected \
                 ({stored_model} {stored_dims}d -> {provider_model} {provider_dims}d), \
                 recreating vector table and clearing all embeddings"
            );
            db_guard
                .recreate_vectors_table(provider_dims)
                .context("Failed to recreate vectors table for new model")?;
            db_guard
                .set_embedding_config(&provider_model, provider_dims)
                .context("Failed to update embedding config")?;
        }
    }

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

    // Build base prepared symbols (existing embeddable kinds) and merge selected variables.
    let base_prepared = prepare_batch_for_embedding(&symbols, lang_configs);
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
    let variable_budget_cap =
        ((base_prepared.len() as f64) * VARIABLE_EMBEDDING_POLICY.max_ratio).floor() as usize;

    info!(
        "Embedding pipeline variable policy: candidate_count={}, selected_count={}, budget_cap={}",
        candidate_variable_ids.len(),
        selected_variable_ids.len(),
        variable_budget_cap,
    );

    let mut all_prepared = base_prepared;
    all_prepared.extend(selected_variables);
    if all_prepared.is_empty() {
        info!("Embedding pipeline: no embeddable symbols found, skipping");
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
                warn!(
                    "Embedding pipeline: failed to delete stale embeddings, continuing: {err:#}"
                );
                0
            }
        }
    };
    if stale_deleted > 0 {
        info!("Embedding pipeline: purged {stale_deleted} stale embeddings");
    }

    // Container symbols always get fresh embeddings because their text includes
    // child method names (via enrichment), which change when children are added/removed.
    let container_ids: HashSet<&str> = symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface | SymbolKind::Trait
            )
        })
        .map(|s| s.id.as_str())
        .collect();

    // Skip symbols that already have embeddings (incremental),
    // EXCEPT container symbols which always get re-embedded with enriched child names.
    let prepared: Vec<_> = all_prepared
        .into_iter()
        .filter(|(id, _)| !already_embedded.contains(id) || container_ids.contains(id.as_str()))
        .collect();

    stats.symbols_skipped = already_embedded.len();

    if prepared.is_empty() {
        info!(
            "Embedding pipeline: all {} embeddable symbols already embedded, nothing to do",
            stats.symbols_skipped
        );
        return Ok(stats);
    }

    info!(
        "Embedding pipeline: {} new symbols to embed ({} already embedded, {} total)",
        prepared.len(),
        stats.symbols_skipped,
        symbols.len()
    );

    // Process in batches
    for chunk in prepared.chunks(EMBEDDING_BATCH_SIZE) {
        let texts: Vec<String> = chunk.iter().map(|(_, text)| text.clone()).collect();

        // Generate embeddings — if a batch fails (e.g., DirectML RuntimeError),
        // log the error and stop. Successful batches are already persisted, and
        // the incremental filter will pick up missed symbols on the next run.
        let total_batches = (prepared.len() + EMBEDDING_BATCH_SIZE - 1) / EMBEDDING_BATCH_SIZE;
        let vectors = match provider.embed_batch(&texts) {
            Ok(v) => v,
            Err(err) => {
                warn!(
                    "Embedding batch {}/{total_batches} failed ({} texts), \
                     stopping pipeline ({} embeddings stored so far): {err:#}",
                    stats.batches_processed + 1,
                    texts.len(),
                    stats.symbols_embedded,
                );
                break;
            }
        };

        if vectors.len() != chunk.len() {
            warn!(
                "Embedding count mismatch: expected {}, got {}",
                chunk.len(),
                vectors.len()
            );
            continue;
        }

        // Pair symbol_ids with their vectors
        let pairs: Vec<(String, Vec<f32>)> = chunk
            .iter()
            .zip(vectors.into_iter())
            .map(|((id, _), vec)| (id.clone(), vec))
            .collect();

        // Store in database
        let stored = {
            let mut db_guard = db
                .lock()
                .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
            db_guard
                .store_embeddings(&pairs)
                .context("Failed to store embeddings")?
        };

        stats.symbols_embedded += stored;
        stats.batches_processed += 1;

        info!(
            "Embedding batch {}/{}: stored {stored} embeddings ({} total so far)",
            stats.batches_processed,
            (prepared.len() + EMBEDDING_BATCH_SIZE - 1) / EMBEDDING_BATCH_SIZE,
            stats.symbols_embedded,
        );
    }

    info!(
        "Embedding pipeline complete: {}/{} symbols embedded in {} batches",
        stats.symbols_embedded, stats.symbols_scanned, stats.batches_processed
    );

    Ok(stats)
}

/// Embed symbols for a single file (used by incremental indexer after file changes).
///
/// This is synchronous and fast (<200ms for typical files with 5-50 embeddable symbols).
pub fn embed_symbols_for_file(
    db: &Arc<Mutex<SymbolDatabase>>,
    provider: &dyn EmbeddingProvider,
    file_path: &str,
    lang_configs: Option<&LanguageConfigs>,
) -> Result<usize> {
    // Load symbols for this file
    let symbols = {
        let db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        db_guard
            .get_symbols_for_file(file_path)
            .context("Failed to load symbols for file")?
    };

    // Filter and format structural symbols only.
    // Variable embedding is handled globally by `run_embedding_pipeline` at workspace init
    // using budgeted selection. The incremental path skips variables to stay fast (<200ms).
    let prepared = prepare_batch_for_embedding(&symbols, lang_configs);
    if prepared.is_empty() {
        return Ok(0);
    }

    let texts: Vec<String> = prepared.iter().map(|(_, text)| text.clone()).collect();

    // Generate embeddings
    let vectors = provider
        .embed_batch(&texts)
        .context("Failed to embed file symbols")?;

    if vectors.len() != prepared.len() {
        bail!(
            "Embedding count mismatch for file {}: expected {}, got {}",
            file_path,
            prepared.len(),
            vectors.len()
        );
    }

    // Pair and store
    let pairs: Vec<(String, Vec<f32>)> = prepared
        .iter()
        .zip(vectors.into_iter())
        .map(|((id, _), vec)| (id.clone(), vec))
        .collect();

    let mut db_guard = db
        .lock()
        .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
    db_guard
        .store_embeddings(&pairs)
        .context("Failed to store file embeddings")
}

/// Re-embed all symbols for a file, replacing any stale vectors for that file.
///
/// Use this after incremental re-indexing on create/modify events where symbol IDs
/// may have changed and old vectors must be removed first.
pub fn reembed_symbols_for_file(
    db: &Arc<Mutex<SymbolDatabase>>,
    provider: &dyn EmbeddingProvider,
    file_path: &str,
    lang_configs: Option<&LanguageConfigs>,
) -> Result<usize> {
    {
        let mut db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        db_guard
            .delete_embeddings_for_file(file_path)
            .context("Failed to delete stale file embeddings before re-embed")?;
        db_guard
            .delete_orphan_embeddings()
            .context("Failed to delete orphan embeddings before re-embed")?;
    }

    embed_symbols_for_file(db, provider, file_path, lang_configs)
}
