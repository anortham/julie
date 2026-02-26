//! Background embedding pipeline.
//!
//! After Tantivy indexing completes, this pipeline runs in the background to
//! generate vector embeddings for all embeddable symbols. It processes symbols
//! in batches to avoid holding the database lock for too long.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::metadata::prepare_batch_for_embedding;
use crate::embeddings::EmbeddingProvider;

/// Batch size for embedding generation (symbols per batch).
const EMBEDDING_BATCH_SIZE: usize = 500;

/// Statistics from an embedding pipeline run.
#[derive(Debug, Clone)]
pub struct EmbeddingStats {
    pub symbols_scanned: usize,
    pub symbols_embedded: usize,
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
        batches_processed: 0,
    };

    // Load all symbols from the database
    let symbols = {
        let db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        db_guard
            .get_all_symbols()
            .context("Failed to load symbols for embedding")?
    };

    stats.symbols_scanned = symbols.len();
    info!(
        "Embedding pipeline: {} total symbols loaded",
        symbols.len()
    );

    // Filter to embeddable kinds and format metadata
    let prepared = prepare_batch_for_embedding(&symbols);
    if prepared.is_empty() {
        info!("Embedding pipeline: no embeddable symbols found, skipping");
        return Ok(stats);
    }

    info!(
        "Embedding pipeline: {} embeddable symbols (of {} total)",
        prepared.len(),
        symbols.len()
    );

    // Process in batches
    for chunk in prepared.chunks(EMBEDDING_BATCH_SIZE) {
        let texts: Vec<String> = chunk.iter().map(|(_, text)| text.clone()).collect();

        // Generate embeddings
        let vectors = provider
            .embed_batch(&texts)
            .context("Embedding batch failed")?;

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

        debug!(
            "Embedding batch {}: stored {stored} embeddings",
            stats.batches_processed
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
