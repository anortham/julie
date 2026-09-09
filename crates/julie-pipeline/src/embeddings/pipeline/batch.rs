use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use tracing::{info, warn};

use super::{EMBEDDING_BATCH_SIZE, EmbeddingStats};
use crate::embeddings::{EmbeddingProvider, EmbeddingRequestBudget};
use julie_core::database::SymbolDatabase;

/// Process prepared symbol batches: chunk, embed, check budgets and cancellation, and persist vectors.
pub(crate) fn process_embedding_batches(
    db: &Arc<Mutex<SymbolDatabase>>,
    provider: &dyn EmbeddingProvider,
    prepared: &[(String, String)],
    generation_id: i64,
    cancel_token: &Arc<std::sync::atomic::AtomicBool>,
    stats: &mut EmbeddingStats,
) -> Result<bool> {
    let mut completed_cleanly = true;
    let total_batches = (prepared.len() + EMBEDDING_BATCH_SIZE - 1) / EMBEDDING_BATCH_SIZE;
    for chunk in prepared.chunks(EMBEDDING_BATCH_SIZE) {
        // Check cancellation between batches (e.g., force reindex aborts old pipeline)
        if cancel_token.load(std::sync::atomic::Ordering::Acquire) {
            info!(
                "Embedding pipeline cancelled after {} batches ({} embeddings stored)",
                stats.batches_processed, stats.symbols_embedded
            );
            completed_cleanly = false;
            break;
        }

        let texts: Vec<String> = chunk.iter().map(|(_, text)| text.clone()).collect();

        let batch_budget = EmbeddingRequestBudget {
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
            cancelled: Arc::clone(cancel_token),
        };

        // Generate embeddings -- if a batch fails (e.g., DirectML RuntimeError),
        // log the error and stop. Successful batches are already persisted, and
        // the incremental filter will pick up missed symbols on the next run.
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
                completed_cleanly = false;
                break;
            }
        };

        // Pre-storage cancellation and deadline check.
        // If cancellation was signalled or deadline expired during provider inference,
        // do not acquire the DB lock or store embeddings for this batch.
        if cancel_token.load(std::sync::atomic::Ordering::Acquire) || batch_budget.is_cancelled() {
            info!(
                "Embedding pipeline cancelled during batch inference, skipping DB write ({} embeddings stored so far)",
                stats.symbols_embedded
            );
            completed_cleanly = false;
            break;
        }
        if batch_budget.is_expired() {
            warn!(
                "Embedding batch deadline exceeded during inference, skipping DB write ({} embeddings stored so far)",
                stats.symbols_embedded
            );
            completed_cleanly = false;
            break;
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
                .store_embeddings_for_generation(generation_id, &pairs)
                .context("Failed to store embeddings")?
        };

        stats.symbols_embedded += stored;
        stats.batches_processed += 1;

        // Post-batch cancel check. The pre-batch check catches cancellations
        // set before the batch started; this check catches cancellations set DURING embed_batch
        // (e.g., a concurrent force-reindex), stopping before the next batch begins.
        if cancel_token.load(std::sync::atomic::Ordering::Acquire) {
            info!(
                "Embedding pipeline cancelled after batch write ({} embeddings stored)",
                stats.symbols_embedded
            );
            completed_cleanly = false;
            break;
        }

        info!(
            "Embedding batch {}/{}: stored {stored} embeddings ({} total so far)",
            stats.batches_processed,
            (prepared.len() + EMBEDDING_BATCH_SIZE - 1) / EMBEDDING_BATCH_SIZE,
            stats.symbols_embedded,
        );
    }

    Ok(completed_cleanly)
}
