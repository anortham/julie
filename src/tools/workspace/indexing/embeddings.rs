//! Embedding helpers for workspace indexing.
//!
//! Spawns the embedding pipeline for any registered workspace (primary or
//! reference) using the active embedding provider and a fresh database
//! connection.

use std::sync::{Arc, Mutex};

use tracing::{debug, info, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::embeddings::pipeline::run_embedding_pipeline_cancellable;
use crate::handler::JulieServerHandler;

/// Outcome of `spawn_workspace_embedding`.
///
/// - `symbols`: count of symbols in the target workspace DB. Caller uses this
///   to format response messages. `0` indicates embedding was skipped.
/// - `deferred`: `true` when the daemon embedding provider is still bootstrapping
///   and the actual pipeline run was queued in a background task. The index
///   response should not wait for it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EmbeddingOutcome {
    pub symbols: usize,
    pub deferred: bool,
}

impl EmbeddingOutcome {
    fn skipped() -> Self {
        Self {
            symbols: 0,
            deferred: false,
        }
    }
}

/// Spawn the embedding pipeline for a workspace (fire-and-forget).
///
/// Returns an [`EmbeddingOutcome`] so the caller can include the symbol count
/// (and whether the run was deferred behind a still-initializing provider) in
/// response messages. Returns `symbols: 0` if embedding is skipped (no
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
    // Fast path: check handler (daemon shared service or workspace provider)
    let provider = if let Some(p) = handler.embedding_provider().await {
        p
    } else if let Some(svc) = handler.embedding_service.as_ref() {
        // Daemon mode. The shared service may still be in `Initializing`
        // (background bootstrap of the Python sidecar + torch + model load
        // takes ~36-39s on cold start). Probe non-blocking; if it's already
        // settled, use the result immediately. Otherwise hand off to a
        // deferred task that waits for settlement without blocking the
        // index response.
        use crate::daemon::embedding_service::EmbeddingServiceSettled;
        match svc.try_settled() {
            Some(EmbeddingServiceSettled::Ready { provider: p, .. }) => {
                debug!("Daemon embedding service Ready; proceeding inline");
                p
            }
            Some(EmbeddingServiceSettled::Unavailable { reason, .. }) => {
                debug!(
                    %reason,
                    "Daemon embedding service Unavailable; skipping workspace embedding"
                );
                return EmbeddingOutcome::skipped();
            }
            Some(EmbeddingServiceSettled::Timeout) => {
                // try_settled never returns Timeout; defensive fall-through.
                return EmbeddingOutcome::skipped();
            }
            None => {
                // Still initializing. Queue a deferred task and return now.
                return spawn_deferred_daemon_embedding(handler, workspace_id).await;
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
            return EmbeddingOutcome::skipped();
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

    let db_path = match handler.workspace_db_file_path_for(&workspace_id).await {
        Ok(path) => path,
        Err(e) => {
            warn!("Failed to resolve workspace DB path for embedding: {e}");
            return EmbeddingOutcome::skipped();
        }
    };
    if !db_path.exists() {
        warn!("Target workspace DB not found at {}", db_path.display());
        return EmbeddingOutcome::skipped();
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
            return EmbeddingOutcome::skipped();
        }
        Err(e) => {
            warn!("Workspace DB open task panicked: {e}");
            return EmbeddingOutcome::skipped();
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
        run_pipeline_body(
            provider,
            db_arc,
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
        deferred: false,
    }
}

/// Shared embedding-pipeline body. Runs the cancellable pipeline against an
/// already-resolved DB + provider, then updates daemon.db and cleans up the
/// task slot. Used by both the inline (fast) path and the deferred daemon path.
async fn run_pipeline_body(
    provider: Arc<dyn EmbeddingProvider>,
    db_arc: Arc<Mutex<SymbolDatabase>>,
    workspace_id: String,
    cancel_for_pipeline: Arc<std::sync::atomic::AtomicBool>,
    self_cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    daemon_db: Option<Arc<crate::daemon::database::DaemonDatabase>>,
    embedding_task_slot: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<
                String,
                (
                    Arc<std::sync::atomic::AtomicBool>,
                    tokio::task::JoinHandle<()>,
                ),
            >,
        >,
    >,
    total_symbols: usize,
) {
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
}

/// Queue an embedding run that waits for the daemon's shared service to
/// settle out of `Initializing`. Returns immediately with `deferred: true`
/// so the index response is not blocked on the sidecar bootstrap.
///
/// The spawned task registers itself in `handler.embedding_tasks` before
/// the wait so a subsequent force-reindex can cancel it. On `Unavailable`
/// or `Timeout`, the task logs and exits, cleaning up its slot.
async fn spawn_deferred_daemon_embedding(
    handler: &JulieServerHandler,
    workspace_id: String,
) -> EmbeddingOutcome {
    use crate::daemon::embedding_service::EmbeddingServiceSettled;

    let svc = match handler.embedding_service.as_ref() {
        Some(s) => Arc::clone(s),
        None => return EmbeddingOutcome::skipped(),
    };

    // Resolve DB path eagerly (uses handler; fast). The path may not exist yet
    // if a fresh index is still being written, but the deferred task waits
    // ~tens of seconds for the sidecar so it should exist by the time we open it.
    let db_path = match handler.workspace_db_file_path_for(&workspace_id).await {
        Ok(path) => path,
        Err(e) => {
            warn!("Failed to resolve workspace DB path for deferred embedding: {e}");
            return EmbeddingOutcome::skipped();
        }
    };

    let daemon_db = handler.daemon_db.clone();
    let embedding_tasks = handler.embedding_tasks.clone();

    // Cancel any previously running pipeline for this workspace, mirroring
    // the fast-path semantics so a deferred run replaces a stale one.
    {
        let mut tasks = embedding_tasks.lock().await;
        if let Some((prev_flag, prev_handle)) = tasks.remove(&workspace_id) {
            info!(
                "Cancelling previous embedding pipeline for workspace {workspace_id} before deferred run"
            );
            prev_flag.store(true, std::sync::atomic::Ordering::Release);
            prev_handle.abort();
        }
    }

    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_for_task = cancel_flag.clone();
    let workspace_id_for_task = workspace_id.clone();
    let embedding_tasks_for_task = embedding_tasks.clone();

    let handle = tokio::spawn(async move {
        info!(
            workspace_id = %workspace_id_for_task,
            "Deferred embedding: waiting up to 120s for daemon embedding service to settle"
        );

        let provider = match svc
            .wait_until_settled(std::time::Duration::from_secs(120))
            .await
        {
            EmbeddingServiceSettled::Ready { provider, .. } => provider,
            EmbeddingServiceSettled::Unavailable { reason, .. } => {
                debug!(
                    %reason,
                    workspace_id = %workspace_id_for_task,
                    "Deferred embedding: service settled to Unavailable; skipping"
                );
                cleanup_task_slot(
                    &embedding_tasks_for_task,
                    &workspace_id_for_task,
                    &cancel_for_task,
                )
                .await;
                sync_vector_count_on_terminal(&daemon_db, &workspace_id_for_task, &db_path).await;
                return;
            }
            EmbeddingServiceSettled::Timeout => {
                warn!(
                    workspace_id = %workspace_id_for_task,
                    "Deferred embedding: service did not settle within 120s; skipping"
                );
                cleanup_task_slot(
                    &embedding_tasks_for_task,
                    &workspace_id_for_task,
                    &cancel_for_task,
                )
                .await;
                sync_vector_count_on_terminal(&daemon_db, &workspace_id_for_task, &db_path).await;
                return;
            }
        };

        if cancel_for_task.load(std::sync::atomic::Ordering::Acquire) {
            info!(
                workspace_id = %workspace_id_for_task,
                "Deferred embedding cancelled before pipeline start"
            );
            cleanup_task_slot(
                &embedding_tasks_for_task,
                &workspace_id_for_task,
                &cancel_for_task,
            )
            .await;
            return;
        }

        if !db_path.exists() {
            warn!(
                "Deferred embedding: workspace DB not found at {}",
                db_path.display()
            );
            cleanup_task_slot(
                &embedding_tasks_for_task,
                &workspace_id_for_task,
                &cancel_for_task,
            )
            .await;
            // DB missing: vector count is definitionally 0.
            sync_vector_count_on_terminal(&daemon_db, &workspace_id_for_task, &db_path).await;
            return;
        }

        let db = match tokio::task::spawn_blocking({
            let path = db_path.clone();
            move || SymbolDatabase::new(path)
        })
        .await
        {
            Ok(Ok(db)) => db,
            Ok(Err(e)) => {
                warn!("Deferred embedding: failed to open workspace DB: {e}");
                cleanup_task_slot(
                    &embedding_tasks_for_task,
                    &workspace_id_for_task,
                    &cancel_for_task,
                )
                .await;
                // DB unreadable: treat vector count as 0.
                sync_vector_count_on_terminal(&daemon_db, &workspace_id_for_task, &db_path).await;
                return;
            }
            Err(e) => {
                warn!("Deferred embedding: workspace DB open task panicked: {e}");
                cleanup_task_slot(
                    &embedding_tasks_for_task,
                    &workspace_id_for_task,
                    &cancel_for_task,
                )
                .await;
                // DB open panicked: treat vector count as 0.
                sync_vector_count_on_terminal(&daemon_db, &workspace_id_for_task, &db_path).await;
                return;
            }
        };

        let total_symbols = db
            .get_stats()
            .map(|s| s.total_symbols as usize)
            .unwrap_or(0);

        let db_arc = Arc::new(Mutex::new(db));

        run_pipeline_body(
            provider,
            db_arc,
            workspace_id_for_task,
            cancel_for_task.clone(),
            cancel_for_task,
            daemon_db,
            embedding_tasks_for_task,
            total_symbols,
        )
        .await;
    });

    // Register BEFORE returning so subsequent force-reindex callers see the slot.
    {
        let mut tasks = embedding_tasks.lock().await;
        tasks.insert(workspace_id, (cancel_flag, handle));
    }

    EmbeddingOutcome {
        symbols: 0,
        deferred: true,
    }
}

/// After a deferred embedding task exits on a terminal path (Unavailable,
/// Timeout, missing DB, DB-open failure), sync the actual vector count into
/// `daemon.db` so it doesn't show stale numbers left over from a prior run
/// (e.g. after force-reindex cleared embeddings).
pub(crate) async fn sync_vector_count_on_terminal(
    daemon_db: &Option<Arc<crate::daemon::database::DaemonDatabase>>,
    workspace_id: &str,
    db_path: &std::path::Path,
) {
    if let Some(daemon) = daemon_db {
        let actual_count = if db_path.exists() {
            tokio::task::spawn_blocking({
                let path = db_path.to_path_buf();
                move || {
                    SymbolDatabase::new(path)
                        .and_then(|db| db.embedding_count())
                        .unwrap_or(0)
                }
            })
            .await
            .unwrap_or(0)
        } else {
            0
        };
        let _ = daemon.update_vector_count(workspace_id, actual_count);
    }
}

/// Remove a task slot from the embedding_tasks map only if the stored cancel
/// flag is still the one this task created (defensive against newer overwrites).
async fn cleanup_task_slot(
    embedding_tasks: &Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<
                String,
                (
                    Arc<std::sync::atomic::AtomicBool>,
                    tokio::task::JoinHandle<()>,
                ),
            >,
        >,
    >,
    workspace_id: &str,
    self_cancel_flag: &Arc<std::sync::atomic::AtomicBool>,
) {
    let mut tasks = embedding_tasks.lock().await;
    if let Some((stored_flag, _)) = tasks.get(workspace_id) {
        if Arc::ptr_eq(stored_flag, self_cancel_flag) {
            tasks.remove(workspace_id);
        }
    }
}
