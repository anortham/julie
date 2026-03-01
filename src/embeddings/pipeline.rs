//! Background embedding pipeline.
//!
//! After Tantivy indexing completes, this pipeline runs in the background to
//! generate vector embeddings for all embeddable symbols. It processes symbols
//! in batches to avoid holding the database lock for too long.

use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use tracing::{info, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::metadata::{prepare_batch_for_embedding, NON_EMBEDDABLE_LANGUAGES};
use crate::embeddings::EmbeddingProvider;
use crate::extractors::SymbolKind;

/// Batch size for embedding generation (symbols per batch).
const EMBEDDING_BATCH_SIZE: usize = 500;

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
) -> Result<EmbeddingStats> {
    let mut stats = EmbeddingStats {
        symbols_scanned: 0,
        symbols_embedded: 0,
        symbols_skipped: 0,
        batches_processed: 0,
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
            info!(
                "Embedding pipeline: purged {purged} non-code embeddings (markdown, json, etc.)"
            );
        }
    }

    // Load all symbols and existing embedding IDs from the database
    let (symbols, already_embedded) = {
        let db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        let syms = db_guard
            .get_all_symbols()
            .context("Failed to load symbols for embedding")?;
        let embedded = db_guard
            .get_embedded_symbol_ids()
            .context("Failed to load existing embedding IDs")?;
        (syms, embedded)
    };

    stats.symbols_scanned = symbols.len();
    info!("Embedding pipeline: {} total symbols loaded", symbols.len());

    // Filter to embeddable kinds and format metadata
    let all_prepared = prepare_batch_for_embedding(&symbols);
    if all_prepared.is_empty() {
        info!("Embedding pipeline: no embeddable symbols found, skipping");
        return Ok(stats);
    }

    // Container symbols always get fresh embeddings because their text includes
    // child method names (via enrichment), which change when children are added/removed.
    let container_ids: std::collections::HashSet<&str> = symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Class
                    | SymbolKind::Struct
                    | SymbolKind::Interface
                    | SymbolKind::Trait
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
        let total_batches =
            (prepared.len() + EMBEDDING_BATCH_SIZE - 1) / EMBEDDING_BATCH_SIZE;
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

    // Filter and format
    let prepared = prepare_batch_for_embedding(&symbols);
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

    embed_symbols_for_file(db, provider, file_path)
}
