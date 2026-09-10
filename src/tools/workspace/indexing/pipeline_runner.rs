//! Background embedding pipeline runner.

use std::sync::Arc;

use julie_index::checkout_store::CheckoutStore;
use tracing::{info, warn};

use crate::embeddings::EmbeddingProvider;
use crate::embeddings::pipeline::run_embedding_pipeline_cancellable;

/// Shared embedding-pipeline body. Runs the cancellable pipeline against the
/// workspace's checkout store, then updates daemon.db and cleans up the
/// task slot.
pub(crate) async fn run_pipeline_body(
    provider: Arc<dyn EmbeddingProvider>,
    store: Arc<CheckoutStore>,
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
    let store_for_run = Arc::clone(&store);
    let lang_configs = crate::search::language_config::LanguageConfigs::load_embedded();
    // Capture model name before provider is moved into spawn_blocking
    let model_name = provider.device_info().model_name.clone();
    let result = tokio::task::spawn_blocking(move || {
        run_embedding_pipeline_cancellable(
            &store_for_run,
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

    // Runs after every outcome (success, failure, cancellation) so daemon.db
    // reports the store's bound vector count, not this run's delta.
    if let Some(ref daemon) = daemon_db {
        let actual_count = store.status().vector_count as i64;
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
