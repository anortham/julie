//! Embedding helpers for workspace indexing.
//!
//! Spawns the embedding pipeline for any registered workspace (primary or
//! reference) using the active embedding provider and the workspace's
//! checkout store.

use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::handler::JulieServerHandler;

/// Outcome of `spawn_workspace_embedding`.
///
/// `symbols` is the count of symbols in the target workspace snapshot. Callers use
/// it to format response messages; `0` means embedding was skipped.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EmbeddingOutcome {
    pub symbols: usize,
}

impl EmbeddingOutcome {
    pub(crate) fn skipped() -> Self {
        Self { symbols: 0 }
    }
}

/// Vectors bound to the workspace's current snapshot; zero when the store
/// cannot be opened.
pub(crate) async fn workspace_vector_coverage(
    handler: &JulieServerHandler,
    workspace_id: &str,
) -> (usize, usize) {
    let Ok(root) = handler.get_workspace_root_for_target(workspace_id).await else {
        return (0, 0);
    };
    match handler
        .checkout_store_for_workspace(workspace_id, &root)
        .await
    {
        Ok(store) => julie_pipeline::embeddings::pipeline::eligible_vector_coverage(
            store.current().as_ref(),
            Some(&crate::search::language_config::LanguageConfigs::load_embedded()),
        ),
        Err(_) => (0, 0),
    }
}

/// Spawn the embedding pipeline for a workspace (fire-and-forget).
///
/// Returns an [`EmbeddingOutcome`] so the caller can include the symbol count
/// in response messages. Returns `symbols: 0` if embedding is skipped (no
/// provider, no workspace, etc.).
///
/// If the embedding provider has not been initialized yet (deferred from
/// workspace startup to avoid blocking indexing), this function either
/// initializes it inline (stdio mode) or queues a deferred task that waits
/// for the daemon's shared service to settle (daemon mode) so the caller
/// returns immediately.
pub(crate) async fn spawn_workspace_embedding(
    handler: &JulieServerHandler,
    workspace_id: String,
) -> EmbeddingOutcome {
    let Some(provider) = handler
        .acquire_embedding_provider(Duration::from_secs(30))
        .await
    else {
        return EmbeddingOutcome::skipped();
    };

    let root = match handler.get_workspace_root_for_target(&workspace_id).await {
        Ok(root) => root,
        Err(e) => {
            warn!("Failed to resolve workspace root for embedding: {e}");
            return EmbeddingOutcome::skipped();
        }
    };
    let store = match handler
        .checkout_store_for_workspace(&workspace_id, &root)
        .await
    {
        Ok(store) => store,
        Err(e) => {
            warn!("Failed to open workspace store for embedding: {e}");
            return EmbeddingOutcome::skipped();
        }
    };
    let lang_configs = crate::search::language_config::LanguageConfigs::load_embedded();
    let total_symbols = julie_pipeline::embeddings::pipeline::select_eligible_symbol_ids(
        store.current().graph(),
        Some(&lang_configs),
    )
    .len();

    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_for_pipeline = cancel_flag.clone();
    let daemon_db = handler.daemon_db.clone();
    let workspace_id_for_store = workspace_id.clone();
    let embedding_task_slot = handler.embedding_tasks.clone();
    let self_cancel_flag = cancel_flag.clone();
    {
        let mut tasks = handler.embedding_tasks.lock().await;
        if tasks.contains_key(&workspace_id_for_store) {
            return EmbeddingOutcome::skipped();
        }
        let handle = tokio::spawn(async move {
            super::pipeline_runner::run_pipeline_body(
                provider,
                store,
                workspace_id,
                cancel_for_pipeline,
                self_cancel_flag,
                daemon_db,
                embedding_task_slot,
                total_symbols,
            )
            .await;
        });
        tasks.insert(workspace_id_for_store.clone(), (cancel_flag, handle));
    }

    EmbeddingOutcome {
        symbols: total_symbols,
    }
}
