//! Background embedding pipeline runner.

use std::sync::{Arc, Mutex};
use tracing::{info, warn};

use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::embeddings::pipeline::run_embedding_pipeline_cancellable;

/// Shared embedding-pipeline body. Runs the cancellable pipeline against an
/// already-resolved DB + provider, then updates daemon.db and cleans up the
/// task slot.
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
