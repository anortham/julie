use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tracing::warn;

use super::enrichment::{build_callee_map, build_field_access_map};
use crate::embeddings::metadata::prepare_batch_for_embedding;
use crate::embeddings::{EmbeddingProvider, EmbeddingRequestBudget};
use julie_core::database::SymbolDatabase;
use julie_index::search::language_config::LanguageConfigs;

/// Embed symbols for a single file (used by incremental indexer after file changes).
///
/// This is synchronous and fast (<200ms for typical files with 5-50 embeddable symbols).
pub fn embed_symbols_for_file(
    db: &Arc<Mutex<SymbolDatabase>>,
    provider: &dyn EmbeddingProvider,
    file_path: &str,
    lang_configs: Option<&LanguageConfigs>,
) -> Result<usize> {
    // Load symbols for this file and capture active ready generation before inference.
    let (symbols, expected_gen_id, expected_source_rev, expected_encoder_key, expected_dims) = {
        let db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let symbols = db_guard
            .get_symbols_for_file(file_path)
            .context("Failed to load symbols for file")?;
        let ready_gen = db_guard.get_latest_ready_generation()?.ok_or_else(|| {
            anyhow::anyhow!("Refusing file embedding: no ready embedding generation exists")
        })?;
        (
            symbols,
            ready_gen.id,
            ready_gen.source_revision,
            ready_gen.encoder_key,
            ready_gen.dimensions,
        )
    };

    if provider.dimensions() != expected_dims {
        anyhow::bail!(
            "Provider dimension mismatch before file embedding: expected {}, got {}",
            expected_dims,
            provider.dimensions()
        );
    }
    let provider_key = provider
        .encoder_identity()
        .and_then(|i| i.storage_key())
        .context("Provider encoder identity is unavailable; missing provenance prevents storing file embeddings")?;
    if provider_key != expected_encoder_key {
        anyhow::bail!(
            "Provider encoder key mismatch before file embedding: expected '{}', got '{}'",
            expected_encoder_key,
            provider_key
        );
    }

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
    let budget = EmbeddingRequestBudget::default();
    let vectors = provider
        .embed_batch(&texts, &budget)
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

    let extra_kinds_per_lang: Vec<(String, Vec<String>)> = if let Some(configs) = lang_configs {
        configs.all_embeddings_extra_kinds()
    } else {
        Vec::new()
    };

    let mut db_guard = db
        .lock()
        .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
    db_guard
        .store_file_embeddings_for_ready_generation_with_extra_kinds(
            file_path,
            &expected_encoder_key,
            expected_gen_id,
            expected_source_rev,
            &pairs,
            &extra_kinds_per_lang,
        )
        .context("Failed to store file embeddings")?;
    Ok(pairs.len())
}

/// Re-embed all symbols for a file, replacing any stale vectors for that file.
///
/// Old vectors are atomically replaced inside `store_file_embeddings_for_ready_generation`.
pub fn reembed_symbols_for_file(
    db: &Arc<Mutex<SymbolDatabase>>,
    provider: &dyn EmbeddingProvider,
    file_path: &str,
    lang_configs: Option<&LanguageConfigs>,
) -> Result<usize> {
    embed_symbols_for_file(db, provider, file_path, lang_configs)
}
