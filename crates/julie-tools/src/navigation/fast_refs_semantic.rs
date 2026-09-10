use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use julie_context::ToolContext;
use julie_core::embeddings_contract::{EmbeddingRequestBudget, SemanticMode};
use julie_index::search::hybrid::compute_tagged_query_embedding_for_hybrid;
use julie_index::search::similarity::{QUERY_SIMILARITY_THRESHOLD, SimilarEntry, similar_to_query};
use julie_index::snapshot::Snapshot;
use tracing::debug;

use super::formatting::format_semantic_fallback;
use crate::snapshot_rows::to_symbol;

const SIMILAR_LIMIT: usize = 5;

/// When zero references are found, try semantic similarity as a fallback:
/// embed the symbol name and scan the snapshot's vector set.
///
/// The sidecar round trip and the scan run on `tokio::task::spawn_blocking`
/// so they never stall a runtime worker thread. An empty vector set, or one
/// built by another encoder, is the not-ready outcome: an error in `Required`
/// mode, an empty section otherwise.
pub async fn try_semantic_fallback(
    symbol: &str,
    handler: &dyn ToolContext,
    snapshot: &Arc<Snapshot>,
    budget: Option<EmbeddingRequestBudget>,
    semantic_mode: SemanticMode,
) -> Result<String> {
    if semantic_mode == SemanticMode::Off {
        return Ok(String::new());
    }

    let effective_budget =
        budget.unwrap_or_else(|| EmbeddingRequestBudget::with_timeout(Duration::from_secs(5)));
    effective_budget.check_budget()?;

    let Some(provider) = handler.embedding_provider().await else {
        if semantic_mode == SemanticMode::Required {
            anyhow::bail!("SEMANTICS_NOT_READY: Embedding provider unavailable");
        }
        return Ok(String::new());
    };

    if snapshot.vectors().is_empty() {
        if semantic_mode == SemanticMode::Required {
            anyhow::bail!("SEMANTICS_NOT_READY: no vectors are published for this workspace");
        }
        debug!("FastRefs semantic fallback for '{symbol}': no vectors published");
        return Ok(String::new());
    }

    let symbol_owned = symbol.to_string();
    let snapshot = Arc::clone(snapshot);
    let similar = tokio::task::spawn_blocking(move || -> Result<Vec<SimilarEntry>> {
        let Some(tagged) = compute_tagged_query_embedding_for_hybrid(
            &symbol_owned,
            Some(provider.as_ref()),
            &effective_budget,
            semantic_mode,
        )?
        else {
            return Ok(Vec::new());
        };
        let encoder_matches = snapshot
            .vectors()
            .encoder()
            .is_some_and(|encoder| encoder.id == tagged.encoder_key);
        if !encoder_matches {
            if semantic_mode == SemanticMode::Required {
                anyhow::bail!(
                    "SEMANTICS_NOT_READY: no vectors for encoder '{}' in this snapshot",
                    tagged.encoder_key
                );
            }
            debug!("FastRefs semantic fallback: published vectors belong to another encoder");
            return Ok(Vec::new());
        }
        let graph = snapshot.graph();
        Ok(similar_to_query(
            &snapshot,
            &tagged.vector,
            SIMILAR_LIMIT,
            QUERY_SIMILARITY_THRESHOLD,
        )
        .into_iter()
        .map(|(id, score)| SimilarEntry {
            symbol: to_symbol(graph, id),
            score,
        })
        .collect())
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking error: {}", e))??;

    Ok(format_semantic_fallback(symbol, &similar))
}
