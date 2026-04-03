//! Background embedding pipeline.
//!
//! After Tantivy indexing completes, this pipeline runs in the background to
//! generate vector embeddings for all embeddable symbols. It processes symbols
//! in batches to avoid holding the database lock for too long.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::embeddings::metadata::{
    NON_EMBEDDABLE_LANGUAGES, VariableEmbeddingPolicy, prepare_batch_for_embedding,
    select_budgeted_variables,
};
use crate::extractors::{RelationshipKind, Symbol, SymbolKind};
use crate::search::language_config::LanguageConfigs;

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
pub const EMBEDDING_FORMAT_VERSION: u32 = 2;

/// Statistics from an embedding pipeline run.
#[derive(Debug, Clone)]
pub struct EmbeddingStats {
    pub symbols_scanned: usize,
    pub symbols_embedded: usize,
    pub symbols_skipped: usize,
    pub batches_processed: usize,
}

/// Build a map of symbol_id -> callee names from the relationship graph.
/// Only includes `Calls` relationships to avoid noise from imports/type refs.
fn build_callee_map(db: &SymbolDatabase, symbols: &[Symbol]) -> HashMap<String, Vec<String>> {
    let func_ids: Vec<String> = symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .map(|s| s.id.clone())
        .collect();

    if func_ids.is_empty() {
        return HashMap::new();
    }

    let relationships = match db.get_outgoing_relationships_for_symbols(&func_ids) {
        Ok(rels) => rels,
        Err(err) => {
            tracing::warn!("Failed to load callees for embedding enrichment: {err:#}");
            return HashMap::new();
        }
    };

    let id_to_name: HashMap<&str, &str> = symbols
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_str()))
        .collect();

    let mut callees: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &relationships {
        if rel.kind == RelationshipKind::Calls {
            if let Some(name) = id_to_name.get(rel.to_symbol_id.as_str()) {
                callees
                    .entry(rel.from_symbol_id.clone())
                    .or_default()
                    .push(name.to_string());
            }
        }
    }

    for names in callees.values_mut() {
        names.sort();
        names.dedup();
    }

    callees
}

/// Build a map of symbol_id -> field access names from the identifiers table.
/// Captures domain vocabulary from member accesses like `self.session_metrics` or `this.db`.
fn build_field_access_map(db: &SymbolDatabase) -> HashMap<String, Vec<String>> {
    match db.get_member_access_identifiers_grouped() {
        Ok(fields) => fields,
        Err(err) => {
            tracing::warn!("Failed to load field accesses for embedding enrichment: {err:#}");
            HashMap::new()
        }
    }
}

/// Build a map of symbol_id -> implementor names from the relationship graph.
/// Finds `Implements` and `Extends` relationships pointing TO trait/interface symbols.
fn build_implementor_map(db: &SymbolDatabase, symbols: &[Symbol]) -> HashMap<String, Vec<String>> {
    let trait_interface_ids: Vec<String> = symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Trait | SymbolKind::Interface))
        .map(|s| s.id.clone())
        .collect();

    if trait_interface_ids.is_empty() {
        return HashMap::new();
    }

    let relationships = match db.get_relationships_to_symbols(&trait_interface_ids) {
        Ok(rels) => rels,
        Err(err) => {
            tracing::warn!("Failed to load implementors for embedding enrichment: {err:#}");
            return HashMap::new();
        }
    };

    let id_to_name: HashMap<&str, &str> = symbols
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_str()))
        .collect();

    let mut implementors: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &relationships {
        if matches!(rel.kind, RelationshipKind::Implements | RelationshipKind::Extends) {
            let impl_name = id_to_name
                .get(rel.from_symbol_id.as_str())
                .copied()
                .unwrap_or(rel.from_symbol_id.as_str());
            implementors
                .entry(rel.to_symbol_id.clone())
                .or_default()
                .push(impl_name.to_string());
        }
    }

    for names in implementors.values_mut() {
        names.sort();
        names.dedup();
        names.truncate(8);
    }

    implementors
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

/// Cancellable variant. When `cancel` is set to `true`, the pipeline stops
/// after the current batch and returns what it has so far.
pub fn run_embedding_pipeline_cancellable(
    db: &Arc<Mutex<SymbolDatabase>>,
    provider: &dyn EmbeddingProvider,
    lang_configs: Option<&LanguageConfigs>,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<EmbeddingStats> {
    let mut stats = EmbeddingStats {
        symbols_scanned: 0,
        symbols_embedded: 0,
        symbols_skipped: 0,
        batches_processed: 0,
    };

    // Detect model/dimension/format changes and recreate the vector table if needed.
    // Any change wipes all vectors and re-embeds from scratch, because vectors from
    // different models or format versions are not comparable.
    {
        let mut db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let (stored_model, stored_dims, stored_fmt_ver) = db_guard
            .get_embedding_config()
            .unwrap_or(("unknown".to_string(), 384, 0));
        let provider_dims = provider.dimensions();
        let provider_model = provider.device_info().model_name;

        if stored_dims != provider_dims || stored_model != provider_model || stored_fmt_ver != EMBEDDING_FORMAT_VERSION {
            let reason = if stored_fmt_ver != EMBEDDING_FORMAT_VERSION {
                format!("format version change (v{stored_fmt_ver} -> v{EMBEDDING_FORMAT_VERSION})")
            } else {
                format!("model change ({stored_model} {stored_dims}d -> {provider_model} {provider_dims}d)")
            };
            info!(
                "Embedding pipeline: {reason}, recreating vector table and clearing all embeddings"
            );
            db_guard
                .recreate_vectors_table(provider_dims)
                .context("Failed to recreate vectors table for new model")?;
            db_guard
                .set_embedding_config(&provider_model, provider_dims, EMBEDDING_FORMAT_VERSION)
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
    //
    // PERF: This unconditionally re-embeds all enriched symbols on every pipeline
    // run (~2000 symbols in a typical workspace), even when enrichment hasn't
    // changed. The correct fix is to store a hash of the enrichment text alongside
    // the vector and only re-embed when it differs. See TODO.md "Embedding format
    // versioning" for the tracking item.
    //
    // For containers: always re-embed (children may have changed).
    // For functions/methods: re-embed if they have callees or field accesses.
    let enriched_ids: HashSet<&str> = symbols
        .iter()
        .filter(|s| match s.kind {
            SymbolKind::Class | SymbolKind::Struct | SymbolKind::Enum => true,
            SymbolKind::Interface | SymbolKind::Trait => {
                // Re-embed when children exist OR implementors have been recorded.
                true
            }
            SymbolKind::Function | SymbolKind::Method => {
                callees_by_symbol.contains_key(&s.id) || fields_by_symbol.contains_key(&s.id)
            }
            _ => false,
        })
        .map(|s| s.id.as_str())
        .collect();

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
            "Embedding pipeline: all {} embeddable symbols already embedded, nothing to do",
            stats.symbols_skipped
        );
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

    // Process in batches
    let total_batches = (prepared.len() + EMBEDDING_BATCH_SIZE - 1) / EMBEDDING_BATCH_SIZE;
    for chunk in prepared.chunks(EMBEDDING_BATCH_SIZE) {
        // Check cancellation between batches (e.g., force reindex aborts old pipeline)
        if cancel.map_or(false, |c| c.load(std::sync::atomic::Ordering::Relaxed)) {
            info!(
                "Embedding pipeline cancelled after {} batches ({} embeddings stored)",
                stats.batches_processed, stats.symbols_embedded
            );
            break;
        }

        let texts: Vec<String> = chunk.iter().map(|(_, text)| text.clone()).collect();

        // Generate embeddings -- if a batch fails (e.g., DirectML RuntimeError),
        // log the error and stop. Successful batches are already persisted, and
        // the incremental filter will pick up missed symbols on the next run.
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

        let usable = vectors.len().min(chunk.len());
        if vectors.len() != chunk.len() {
            warn!(
                "Embedding count mismatch: expected {}, got {}; \
                 storing {usable} partial results (skipped symbols retry on next run)",
                chunk.len(),
                vectors.len(),
            );
        }
        if usable == 0 {
            stats.batches_processed += 1;
            continue;
        }

        // Pair symbol_ids with their vectors (truncate to the smaller of the two)
        let pairs: Vec<(String, Vec<f32>)> = chunk[..usable]
            .iter()
            .zip(vectors.into_iter().take(usable))
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

    // Build callee map for function/method enrichment.
    let (callees_by_symbol, fields_by_symbol) = {
        let db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        (
            build_callee_map(&db_guard, &symbols),
            build_field_access_map(&db_guard),
        )
    };

    // Filter and format structural symbols only.
    // Variable embedding is handled globally by `run_embedding_pipeline` at workspace init
    // using budgeted selection. The incremental path skips variables to stay fast (<200ms).
    // Implementor enrichment is skipped here since this per-file path doesn't have all
    // symbols loaded; the full pipeline handles it on the next workspace-wide run.
    let prepared = prepare_batch_for_embedding(
        &symbols,
        lang_configs,
        &callees_by_symbol,
        &fields_by_symbol,
        &HashMap::new(),
    );
    if prepared.is_empty() {
        return Ok(0);
    }

    let texts: Vec<String> = prepared.iter().map(|(_, text)| text.clone()).collect();

    // Generate embeddings
    let vectors = provider
        .embed_batch(&texts)
        .context("Failed to embed file symbols")?;

    let usable = vectors.len().min(prepared.len());
    if vectors.len() != prepared.len() {
        warn!(
            "Embedding count mismatch for file {}: expected {}, got {}; storing {usable} partial results",
            file_path,
            prepared.len(),
            vectors.len(),
        );
    }
    if usable == 0 {
        return Ok(0);
    }

    // Pair and store
    let pairs: Vec<(String, Vec<f32>)> = prepared[..usable]
        .iter()
        .zip(vectors.into_iter().take(usable))
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use anyhow::Result;

    use super::run_embedding_pipeline;
    use crate::database::SymbolDatabase;
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};

    /// Mock embedding provider that returns one fewer vector than requested,
    /// simulating a partial response from a real provider (e.g., GPU OOM).
    struct PartialProvider {
        dims: usize,
    }

    impl EmbeddingProvider for PartialProvider {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1f32; self.dims])
        }

        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            // Return one fewer vector than requested
            let count = texts.len().saturating_sub(1);
            Ok((0..count).map(|_| vec![0.1f32; self.dims]).collect())
        }

        fn dimensions(&self) -> usize {
            self.dims
        }

        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "partial-mock".to_string(),
                device: "cpu".to_string(),
                model_name: "partial-test-model".to_string(),
                dimensions: self.dims,
            }
        }

        fn shutdown(&self) {}
    }

    fn setup_db_with_functions(count: usize) -> Arc<Mutex<SymbolDatabase>> {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("create db");

        db.conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES ('src/lib.rs', 'rust', 'abc', 100, 0, 0)",
                [],
            )
            .unwrap();

        for i in 0..count {
            db.conn
                .execute(
                    "INSERT INTO symbols (id, name, kind, file_path, language,
                     start_line, start_col, end_line, end_col, start_byte, end_byte,
                     reference_score)
                     VALUES (?, ?, 'function', 'src/lib.rs', 'rust',
                             1, 0, 10, 0, 0, 100, 0.0)",
                    rusqlite::params![format!("sym-{i}"), format!("do_work_{i}")],
                )
                .unwrap();
        }

        // Must forget the dir so the tempfile path stays valid while db is open
        std::mem::forget(dir);
        Arc::new(Mutex::new(db))
    }

    /// When a provider returns fewer vectors than symbols in a batch, the
    /// pipeline should store the partial results rather than skipping the
    /// entire batch.
    ///
    /// Before fix: `continue` skips the batch entirely (0 embeddings stored).
    /// After fix: `min(returned, requested)` embeddings are stored.
    #[test]
    fn test_pipeline_stores_partial_results_on_batch_mismatch() {
        // 3 symbols; PartialProvider returns len-1 = 2 vectors per batch.
        let db = setup_db_with_functions(3);
        let provider = PartialProvider { dims: 4 };

        let stats =
            run_embedding_pipeline(&db, &provider, None).expect("pipeline should not error");

        assert!(
            stats.symbols_embedded > 0,
            "Pipeline must store partial embeddings when provider returns fewer vectors \
             than requested. Got {} embeddings (expected > 0).",
            stats.symbols_embedded
        );
        assert_eq!(
            stats.symbols_embedded, 2,
            "Expected 2 embeddings stored (3 requested - 1 = 2 returned), got {}",
            stats.symbols_embedded
        );
    }
}
