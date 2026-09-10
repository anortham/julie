use std::time::Duration;

use anyhow::Result;
use julie_context::ToolContext;
use julie_core::embeddings_contract::{EmbeddingRequestBudget, SemanticMode};
use julie_index::snapshot::Snapshot;
use tracing::debug;

/// When zero references are found, try semantic similarity as a fallback.
///
/// The snapshot's vector set answers similarity queries; until vectors are
/// published it is empty, so the fallback reports not-ready: an error in
/// `Required` mode, an empty section otherwise.
pub async fn try_semantic_fallback(
    symbol: &str,
    handler: &dyn ToolContext,
    snapshot: &Snapshot,
    budget: Option<EmbeddingRequestBudget>,
    semantic_mode: SemanticMode,
) -> Result<String> {
    if semantic_mode == SemanticMode::Off {
        return Ok(String::new());
    }

    let effective_budget =
        budget.unwrap_or_else(|| EmbeddingRequestBudget::with_timeout(Duration::from_secs(5)));
    effective_budget.check_budget()?;

    if handler.embedding_provider().await.is_none() {
        if semantic_mode == SemanticMode::Required {
            anyhow::bail!("SEMANTICS_NOT_READY: Embedding provider unavailable");
        }
        return Ok(String::new());
    }

    if snapshot.vectors().is_empty() {
        if semantic_mode == SemanticMode::Required {
            anyhow::bail!("SEMANTICS_NOT_READY: no vectors are published for this workspace");
        }
        debug!("FastRefs semantic fallback for '{symbol}': no vectors published");
    }
    Ok(String::new())
}
