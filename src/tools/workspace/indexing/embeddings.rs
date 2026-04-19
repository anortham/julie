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
    } else if let Some(svc) = handler.embedding_service.as_ref() {
        // Daemon mode. The shared service may still be in `Initializing`
        // (background bootstrap of the Python sidecar + torch + model load
        // takes ~36-39s on cold start). Wait up to 120s for it to settle
        // before falling back. 120s is intentionally generous: this is a
        // background indexing task, not a user-facing query, and skipping
        // embedding for a freshly-indexed workspace is far worse than
        // waiting. If the service genuinely fails (publishes Unavailable),
        // we degrade to keyword-only cleanly.
        use crate::daemon::embedding_service::EmbeddingServiceSettled;
        match svc
            .wait_until_settled(std::time::Duration::from_secs(120))
            .await
        {
            EmbeddingServiceSettled::Ready { provider: p, .. } => {
                debug!(
                    "Daemon embedding service became Ready; proceeding with workspace embedding"
                );
                p
            }
            EmbeddingServiceSettled::Unavailable { reason, .. } => {
                debug!(
                    %reason,
                    "Daemon embedding service settled to Unavailable; skipping workspace embedding"
                );
                return 0;
            }
            EmbeddingServiceSettled::Timeout => {
                warn!(
                    "Daemon embedding service did not settle within 120s; skipping workspace embedding"
                );
                return 0;
            }
        }
    } else {
        // Stdio mode: provider not yet initialized. Do it now (deferred from
        // workspace init to avoid blocking symbol extraction and Tantivy indexing).
        let existing_runtime_status = handler.embedding_runtime_status().await;
        if let Some(runtime_status) = existing_runtime_status {
            debug!(
                resolved_backend = %runtime_status.resolved_backend.as_str(),
                accelerated = runtime_status.accelerated,
                degraded_reason = runtime_status.degraded_reason.as_deref().unwrap_or("none"),
                "Embedding runtime already settled without a provider in stdio mode; skipping workspace embedding retry"
            );
            return 0;
        }

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
            // Propagate to file watcher so incremental updates use the new provider
            if let Some(ref watcher) = ws.watcher {
                watcher.update_embedding_provider(ws.embedding_provider.clone());
            }
        }

        match ws.embedding_provider.clone() {
            Some(provider) => provider,
            None => {
                debug!("Embedding provider unavailable after init, skipping workspace embedding");
                return 0;
            }
        }
    };

    let db_path = match handler.workspace_db_file_path_for(&workspace_id).await {
        Ok(path) => path,
        Err(e) => {
            warn!("Failed to resolve workspace DB path for embedding: {e}");
            return 0;
        }
    };
    if !db_path.exists() {
        warn!("Target workspace DB not found at {}", db_path.display());
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
        info!("Starting workspace embedding for {workspace_id} ({total_symbols} symbols)...");
        let db_clone = db_arc.clone();
        let lang_configs = crate::search::language_config::LanguageConfigs::load_embedded();
        // Capture model name before provider is moved into spawn_blocking
        let model_name = provider.device_info().model_name.clone();
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

        // Fix B part 2: unconditionally update daemon.db with the actual vector count.
        // Use embedding_count() (ground-truth DB total) rather than stats.symbols_embedded
        // (this-run delta). Runs after all outcomes: success, failure, and cancellation,
        // so daemon.db never drifts from the workspace DB regardless of pipeline fate.
        if let Some(ref daemon) = daemon_db {
            let actual_count = {
                let db_lock = db_arc.lock().unwrap_or_else(|p| p.into_inner());
                db_lock.embedding_count().unwrap_or(0)
            };
            let _ = daemon.update_vector_count(&workspace_id, actual_count);
            let _ = daemon.update_embedding_model(&workspace_id, &model_name);
        }

        // Clear the stored handle only if it's still ours. A newer pipeline may
        // have replaced the slot between our abort and this cleanup; wiping the
        // newer handle would make it invisible to future cancellation attempts.
        let mut tasks = embedding_task_slot.lock().await;
        if let Some((stored_flag, _)) = tasks.get(&workspace_id) {
            if Arc::ptr_eq(stored_flag, &self_cancel_flag) {
                tasks.remove(&workspace_id);
            }
        }
    });

    // Store the handle + flag so it can be cancelled by a subsequent force reindex
    {
        let mut tasks = handler.embedding_tasks.lock().await;
        tasks.insert(workspace_id_for_store.clone(), (cancel_flag, handle));
    }

    total_symbols
}


