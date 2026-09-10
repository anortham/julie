//! Embedding helpers for workspace indexing.
//!
//! Spawns the embedding pipeline for any registered workspace (primary or
//! reference) using the active embedding provider and the workspace's
//! checkout store.

use std::sync::Arc;

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
    let provider = if let Some(p) = handler.embedding_provider().await {
        p
    } else {
        // Provider not yet initialized. Do it now (deferred from workspace init
        // to avoid blocking symbol extraction and Tantivy indexing).
        let existing_runtime_status = handler.embedding_runtime_status().await;
        if let Some(runtime_status) = existing_runtime_status {
            let retryable = runtime_status
                .degraded_reason
                .as_deref()
                .map_or(false, |r| {
                    r.contains("timeout") || r.contains("unavailable") || r.contains("starting")
                });
            if !retryable {
                debug!(
                    resolved_backend = %runtime_status.resolved_backend.as_str(),
                    accelerated = runtime_status.accelerated,
                    degraded_reason = runtime_status.degraded_reason.as_deref().unwrap_or("none"),
                    "Embedding runtime already settled without a provider in stdio mode; skipping workspace embedding retry"
                );
                return EmbeddingOutcome::skipped();
            }
        }

        info!("Initializing embedding provider (deferred from workspace startup)...");

        let (workspace_identity_root, workspace_for_init) = {
            let ws_guard = handler.workspace.read().await;
            match ws_guard.as_ref() {
                Some(ws) => (ws.root.clone(), ws.clone()),
                None => return EmbeddingOutcome::skipped(),
            }
        };

        // Run heavy provider initialization off runtime worker threads.
        let init_result = tokio::task::spawn_blocking(move || {
            let mut workspace = workspace_for_init;
            workspace.initialize_embedding_provider();
            (
                workspace.embedding_provider.clone(),
                workspace.embedding_runtime_status.clone(),
            )
        })
        .await;

        let (initialized_provider, initialized_runtime_status) = match init_result {
            Ok(result) => result,
            Err(e) => {
                warn!("Embedding provider init task panicked: {e}");
                return EmbeddingOutcome::skipped();
            }
        };

        // Publish initialized state with short write-lock scope.
        let mut ws_guard = handler.workspace.write().await;
        let ws = match ws_guard.as_mut() {
            Some(ws) => ws,
            None => return EmbeddingOutcome::skipped(),
        };

        if ws.root != workspace_identity_root {
            debug!(
                expected_workspace_root = %workspace_identity_root.display(),
                active_workspace_root = %ws.root.display(),
                "Discarding stale embedding init result after workspace switch"
            );
        } else if ws.embedding_provider.is_none() {
            ws.embedding_provider = initialized_provider.clone();
            ws.embedding_runtime_status = initialized_runtime_status;
            // Propagate to file watcher so incremental updates use the new provider
            if let Some(ref watcher) = ws.watcher {
                watcher.update_embedding_provider(ws.embedding_provider.clone());
            }
        }

        match ws.embedding_provider.clone() {
            Some(provider) => provider,
            None => {
                debug!("Embedding provider unavailable after init, skipping workspace embedding");
                return EmbeddingOutcome::skipped();
            }
        }
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
