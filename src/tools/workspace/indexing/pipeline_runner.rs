//! Background pipeline runner and deferred daemon embedding tasks.

use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

use super::embeddings::EmbeddingOutcome;
use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::embeddings::pipeline::run_embedding_pipeline_cancellable;
use crate::handler::JulieServerHandler;

/// Shared embedding-pipeline body. Runs the cancellable pipeline against an
/// already-resolved DB + provider, then updates daemon.db and cleans up the
/// task slot. Used by both the inline (fast) path and the deferred daemon path.
pub(crate) async fn run_pipeline_body(
    provider: Arc<dyn EmbeddingProvider>,
    db_arc: Arc<Mutex<SymbolDatabase>>,
    workspace_id: String,
    cancel_for_pipeline: Arc<std::sync::atomic::AtomicBool>,
    self_cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    daemon_db: Option<Arc<crate::registry::database::DaemonDatabase>>,
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
            Some(cancel_for_pipeline),
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
pub(crate) async fn spawn_deferred_daemon_embedding(
    handler: &JulieServerHandler,
    workspace_id: String,
) -> EmbeddingOutcome {
    use crate::registry::embedding_service::EmbeddingServiceSettled;

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
    daemon_db: &Option<Arc<crate::registry::database::DaemonDatabase>>,
    workspace_id: &str,
    db_path: &Path,
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
