use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use julie_facts::rows::VectorRow;
use julie_index::checkout_store::CheckoutStore;
use tracing::{info, warn};

use super::{EMBEDDING_BATCH_SIZE, EmbeddingStats};
use crate::embeddings::{EmbeddingProvider, EmbeddingRequestBudget};

/// Embed `prepared` (symbol id, text) in chunks and write each chunk's rows
/// through the store, honoring cancellation and the per-batch deadline.
/// Returns whether every batch was written.
pub(crate) fn process_embedding_batches(
    store: &CheckoutStore,
    provider: &dyn EmbeddingProvider,
    prepared: &[(String, String)],
    encoder_id: &str,
    keys: &HashMap<String, (String, u32)>,
    cancel_token: &Arc<AtomicBool>,
    stats: &mut EmbeddingStats,
) -> Result<bool> {
    let total_batches = prepared.len().div_ceil(EMBEDDING_BATCH_SIZE);
    for chunk in prepared.chunks(EMBEDDING_BATCH_SIZE) {
        if cancel_token.load(Ordering::Acquire) {
            info!(
                "Embedding pipeline cancelled after {} batches ({} embeddings stored)",
                stats.batches_processed, stats.symbols_embedded
            );
            return Ok(false);
        }

        let texts: Vec<String> = chunk.iter().map(|(_, text)| text.clone()).collect();
        let batch_budget = EmbeddingRequestBudget {
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
            cancelled: Arc::clone(cancel_token),
        };
        let vectors = match provider.embed_batch(&texts, &batch_budget) {
            Ok(v) => v,
            Err(err) => {
                warn!(
                    "Embedding batch {}/{total_batches} failed ({} texts), \
                     stopping pipeline ({} embeddings stored so far): {err:#}",
                    stats.batches_processed + 1,
                    texts.len(),
                    stats.symbols_embedded,
                );
                return Ok(false);
            }
        };

        if cancel_token.load(Ordering::Acquire) || batch_budget.is_cancelled() {
            info!(
                "Embedding pipeline cancelled during batch inference, skipping write ({} embeddings stored so far)",
                stats.symbols_embedded
            );
            return Ok(false);
        }
        if batch_budget.is_expired() {
            warn!(
                "Embedding batch deadline exceeded during inference, skipping write ({} embeddings stored so far)",
                stats.symbols_embedded
            );
            return Ok(false);
        }

        let usable = vectors.len().min(chunk.len());
        if vectors.len() != chunk.len() {
            warn!(
                "Embedding count mismatch: expected {}, got {}; \
                 storing {usable} partial results (skipped symbols retry on next run)",
                chunk.len(),
                vectors.len(),
            );
        }
        let rows: Vec<VectorRow> = chunk[..usable]
            .iter()
            .zip(vectors.into_iter().take(usable))
            .filter_map(|((id, _), vector)| {
                keys.get(id).map(|(blob_hash, ordinal)| VectorRow {
                    blob_hash: blob_hash.clone(),
                    symbol_ordinal: *ordinal,
                    vector,
                })
            })
            .collect();
        stats.batches_processed += 1;
        if rows.is_empty() {
            continue;
        }

        let stored = store
            .store_vectors(encoder_id, &rows)
            .context("Failed to store embeddings")?;
        stats.symbols_embedded += stored;

        if cancel_token.load(Ordering::Acquire) {
            info!(
                "Embedding pipeline cancelled after batch write ({} embeddings stored)",
                stats.symbols_embedded
            );
            return Ok(false);
        }
        info!(
            "Embedding batch {}/{total_batches}: stored {stored} embeddings ({} total so far)",
            stats.batches_processed, stats.symbols_embedded,
        );
    }
    Ok(true)
}
