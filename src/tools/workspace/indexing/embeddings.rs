//! Embedding helpers for workspace indexing.
//!
//! Spawns the embedding pipeline for any registered workspace (primary or
//! reference) using the active embedding provider and a fresh database
//! connection.

use std::sync::{Arc, Mutex};

use tracing::{debug, info, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::pipeline::run_embedding_pipeline_cancellable;
use crate::handler::JulieServerHandler;

/// Spawn the embedding pipeline for a workspace (fire-and-forget).
///
/// Returns the symbol count so the caller can include it in response messages.
/// Returns 0 if embedding is skipped (no provider, no workspace, etc.).
///
/// If the embedding provider has not been initialized yet (deferred from
/// workspace startup to avoid blocking indexing), this function initializes
/// it here - right before the first embedding run.
pub(crate) async fn spawn_workspace_embedding(
    handler: &JulieServerHandler,
    workspace_id: String,
) -> usize {
    // Fast path: check handler (daemon shared service or workspace provider)
    let provider = if let Some(p) = handler.embedding_provider().await {
        p
    } else if handler.embedding_service.is_some() {
        // Daemon mode but no provider available (e.g., init failed). Don't
        // attempt the stdio lazy-init path; just skip embeddings.
        debug!("Daemon mode but no embedding provider available, skipping workspace embedding");
        return 0;
    } else {
        // Stdio mode: provider not yet initialized. Do it now (deferred from
        // workspace init to avoid blocking symbol extraction and Tantivy indexing).
        info!("Initializing embedding provider (deferred from workspace startup)...");

        let (workspace_identity_root, workspace_for_init) = {
            let ws_guard = handler.workspace.read().await;
            match ws_guard.as_ref() {
                Some(ws) => (ws.root.clone(), ws.clone()),
                None => return 0,
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
                return 0;
            }
        };

        // Publish initialized state with short write-lock scope.
        let mut ws_guard = handler.workspace.write().await;
        let ws = match ws_guard.as_mut() {
            Some(ws) => ws,
            None => return 0,
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
        }

        match ws.embedding_provider.clone() {
            Some(provider) => provider,
            None => {
                debug!(
                    "Embedding provider unavailable after init, skipping workspace embedding"
                );
                return 0;
            }
        }
    };

    let db_path = {
        let ws_guard = handler.workspace.read().await;
        match ws_guard.as_ref() {
            Some(ws) => ws.workspace_db_path(&workspace_id),
            None => return 0,
        }
    };
    if !db_path.exists() {
        warn!("Reference workspace DB not found at {}", db_path.display());
        return 0;
    }

    // Open a fresh database connection in a blocking context
    let db = match tokio::task::spawn_blocking({
        let path = db_path.clone();
        move || SymbolDatabase::new(path)
    })
    .await
    {
        Ok(Ok(db)) => db,
        Ok(Err(e)) => {
            warn!("Failed to open workspace DB for embedding: {e}");
            return 0;
        }
        Err(e) => {
            warn!("Workspace DB open task panicked: {e}");
            return 0;
        }
    };

    // Get symbol count before wrapping in Arc<Mutex> and spawning
    let total_symbols = db
        .get_stats()
        .map(|s| s.total_symbols as usize)
        .unwrap_or(0);

    let db_arc = Arc::new(Mutex::new(db));

    // Cancel and abort any previously running embedding pipeline.
    // Setting the flag stops the spawn_blocking pipeline between batches;
    // aborting the handle kills the outer async wrapper.
    {
        let mut task_guard = handler.embedding_task.lock().await;
        if let Some((cancel_flag, handle)) = task_guard.take() {
            info!("Cancelling previous embedding pipeline before starting new one");
            cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            handle.abort();
        }
    }

    // Create cancellation flag for the new pipeline
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_for_pipeline = cancel_flag.clone();

    // Spawn the pipeline in the background, storing handle + flag for cancellation.
    let embedding_task_slot = handler.embedding_task.clone();
    let self_cancel_flag = cancel_flag.clone();
    let handle = tokio::spawn(async move {
        info!("Starting workspace embedding for {workspace_id} ({total_symbols} symbols)...");
        let db_clone = db_arc.clone();
        let lang_configs = crate::search::language_config::LanguageConfigs::load_embedded();
        let result = tokio::task::spawn_blocking(move || {
            run_embedding_pipeline_cancellable(
                &db_clone,
                provider.as_ref(),
                Some(&lang_configs),
                Some(&cancel_for_pipeline),
            )
        })
        .await;

        match result {
            Ok(Ok(stats)) => {
                info!(
                    "Workspace {workspace_id} embedding complete: {}/{} symbols embedded ({} skipped)",
                    stats.symbols_embedded, stats.symbols_scanned, stats.symbols_skipped
                );
            }
            Ok(Err(e)) => {
                warn!("Workspace {workspace_id} embedding failed: {e:#}");
            }
            Err(e) => {
                if e.is_cancelled() {
                    info!("Workspace {workspace_id} embedding task cancelled");
                } else {
                    warn!("Workspace {workspace_id} embedding task panicked: {e}");
                }
            }
        }

        // Clear the stored handle only if it's still ours. A newer pipeline may
        // have replaced the slot between our abort and this cleanup; wiping the
        // newer handle would make it invisible to future cancellation attempts.
        let mut slot = embedding_task_slot.lock().await;
        if let Some((ref stored_flag, _)) = *slot {
            if Arc::ptr_eq(stored_flag, &self_cancel_flag) {
                *slot = None;
            }
        }
    });

    // Store the handle + flag so it can be cancelled by a subsequent force reindex
    {
        let mut task_guard = handler.embedding_task.lock().await;
        *task_guard = Some((cancel_flag, handle));
    }

    total_symbols
}

/// Backward-compatible wrapper kept for call sites that are reference-specific.
pub(crate) async fn spawn_reference_embedding(
    handler: &JulieServerHandler,
    workspace_id: String,
) -> usize {
    spawn_workspace_embedding(handler, workspace_id).await
}
