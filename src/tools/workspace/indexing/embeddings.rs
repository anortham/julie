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
pub(crate) async fn workspace_vector_count(
    handler: &JulieServerHandler,
    workspace_id: &str,
) -> u64 {
    let Ok(root) = handler.get_workspace_root_for_target(workspace_id).await else {
        return 0;
    };
    match handler
        .checkout_store_for_workspace(workspace_id, &root)
        .await
    {
        Ok(store) => store.status().vector_count,
        Err(_) => 0,
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
    let total_symbols = store.current().graph().len();

    // Cancel and abort any previously running embedding pipeline for this workspace.
    // Setting the flag stops the spawn_blocking pipeline between batches;
    // aborting the handle kills the outer async wrapper.
    {
        let mut tasks = handler.embedding_tasks.lock().await;
        if let Some((cancel_flag, handle)) = tasks.remove(&workspace_id) {
            info!("Cancelling previous embedding pipeline for workspace {workspace_id}");
            cancel_flag.store(true, std::sync::atomic::Ordering::Release);
            handle.abort();
        }
    }

    // Create cancellation flag for the new pipeline
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_for_pipeline = cancel_flag.clone();

    // Capture daemon_db so we can update vector_count on completion
    let daemon_db = handler.daemon_db.clone();

    // Capture workspace_id for the store step (workspace_id is moved into spawn below)
    let workspace_id_for_store = workspace_id.clone();

    // Spawn the pipeline in the background, storing handle + flag for cancellation.
    let embedding_task_slot = handler.embedding_tasks.clone();
    let self_cancel_flag = cancel_flag.clone();
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

    // Store the handle + flag so it can be cancelled by a subsequent force reindex
    {
        let mut tasks = handler.embedding_tasks.lock().await;
        tasks.insert(workspace_id_for_store.clone(), (cancel_flag, handle));
    }

    EmbeddingOutcome {
        symbols: total_symbols,
    }
}
