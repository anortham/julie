use super::{FileChangeEvent, FileChangeType, IncrementalIndexer, SharedEmbeddingProvider};
use crate::workspace::mutation_gate::Registry as MutationGateRegistry;
use anyhow::Result;
use julie_core::indexing_state::SharedIndexingRuntime;
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_index::checkout_store::CheckoutStore;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, info, warn};

mod processing;

const EXTRACTOR_REPAIR_RETRY_INTERVAL: Duration = Duration::from_secs(30);
/// Maximum number of times a single file's Tantivy projection retry can fail
/// before we abandon retrying it. With a 1-second retry tick this means we
/// stop after ~10 seconds, which is long enough to ride out transient
/// filesystem hiccups but short enough to stop log spam when the index
/// directory has been deleted out from under the daemon.
const MAX_TANTIVY_RETRY_ATTEMPTS: u32 = 10;

#[derive(Clone)]
pub(super) struct QueueRuntime {
    store: Arc<CheckoutStore>,
    embedding_provider: SharedEmbeddingProvider,
    lang_configs: Arc<julie_index::search::language_config::LanguageConfigs>,
    index_queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    last_processed: Arc<TokioMutex<HashMap<PathBuf, SystemTime>>>,
    supported_extensions: HashSet<String>,
    workspace_root: PathBuf,
    /// Stable workspace identifier used as the mutation-gate key.
    workspace_id: String,
    /// Shared cancellation flag from the owning watcher.
    cancel_flag: Arc<AtomicBool>,
    needs_rescan: Arc<AtomicBool>,
    tantivy_dirty: Arc<StdMutex<HashSet<String>>>,
    /// Per-file failure counter for the dirty-Tantivy retry loop. Once a file
    /// hits MAX_TANTIVY_RETRY_ATTEMPTS we drop it from the dirty set and emit a
    /// single ERROR log instead of spamming WARN every tick.
    tantivy_failure_attempts: Arc<StdMutex<HashMap<String, u32>>>,
    indexing_runtime: SharedIndexingRuntime,
    mutation_gate_registry: Arc<MutationGateRegistry>,
    #[cfg(test)]
    fail_commit_for_test: bool,
    #[cfg(test)]
    rescan_read_failures: Arc<StdMutex<HashSet<String>>>,
    rescan_failed_at: Arc<StdMutex<Option<Instant>>>,
}

impl QueueRuntime {
    pub(super) fn from_indexer(indexer: &IncrementalIndexer) -> Self {
        Self {
            store: Arc::clone(&indexer.store),
            embedding_provider: Arc::clone(&indexer.embedding_provider),
            lang_configs: Arc::clone(&indexer.lang_configs),
            index_queue: Arc::clone(&indexer.index_queue),
            last_processed: Arc::clone(&indexer.last_processed),
            supported_extensions: indexer.supported_extensions.clone(),
            workspace_root: indexer.workspace_root.clone(),
            workspace_id: indexer.workspace_id.clone(),
            cancel_flag: Arc::clone(&indexer.cancel_flag),
            needs_rescan: Arc::clone(&indexer.needs_rescan),
            tantivy_dirty: Arc::clone(&indexer.tantivy_dirty),
            tantivy_failure_attempts: Arc::new(StdMutex::new(HashMap::new())),
            indexing_runtime: Arc::clone(&indexer.indexing_runtime),
            mutation_gate_registry: Arc::clone(&indexer.mutation_gate_registry),
            #[cfg(test)]
            fail_commit_for_test: false,
            #[cfg(test)]
            rescan_read_failures: Arc::clone(&indexer.rescan_read_failures),
            rescan_failed_at: Arc::clone(&indexer.rescan_failed_at),
        }
    }

    pub(super) fn new(
        store: Arc<CheckoutStore>,
        embedding_provider: SharedEmbeddingProvider,
        lang_configs: Arc<julie_index::search::language_config::LanguageConfigs>,
        index_queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
        last_processed: Arc<TokioMutex<HashMap<PathBuf, SystemTime>>>,
        supported_extensions: HashSet<String>,
        workspace_root: PathBuf,
        workspace_id: String,
        cancel_flag: Arc<AtomicBool>,
        needs_rescan: Arc<AtomicBool>,
        tantivy_dirty: Arc<StdMutex<HashSet<String>>>,
        indexing_runtime: SharedIndexingRuntime,
        mutation_gate_registry: Arc<MutationGateRegistry>,
    ) -> Self {
        Self {
            store,
            embedding_provider,
            lang_configs,
            index_queue,
            last_processed,
            supported_extensions,
            workspace_root,
            workspace_id,
            cancel_flag,
            needs_rescan,
            tantivy_dirty,
            tantivy_failure_attempts: Arc::new(StdMutex::new(HashMap::new())),
            indexing_runtime,
            mutation_gate_registry,
            #[cfg(test)]
            fail_commit_for_test: false,
            #[cfg(test)]
            rescan_read_failures: Arc::new(StdMutex::new(HashSet::new())),
            rescan_failed_at: Arc::new(StdMutex::new(None)),
        }
    }

    async fn acquire_gate_or_mark_rescan(&self, context: &str) -> Option<MutationGuard<'static>> {
        if self.cancel_flag.load(Ordering::Acquire) {
            self.mark_rescan_pending_due_to_cancelled_gate(context);
            return None;
        }
        Some(
            self.mutation_gate_registry
                .acquire(&self.workspace_id)
                .await,
        )
    }

    fn mark_rescan_pending_due_to_cancelled_gate(&self, context: &str) {
        self.set_rescan_pending(true);
        warn!(
            workspace_id = %self.workspace_id,
            context,
            "Watcher shutdown skipped queued mutation because the mutation gate was held; rescan marked pending"
        );
    }

    fn set_rescan_pending(&self, pending: bool) {
        self.needs_rescan.store(pending, Ordering::Release);
        self.set_rescan_status(pending);
    }

    fn set_rescan_status(&self, pending: bool) {
        let mut runtime = self
            .indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.set_watcher_rescan_pending(pending);
    }

    fn mark_rescan_failed(&self) {
        *self
            .rescan_failed_at
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Instant::now());
        self.set_rescan_pending(true);
    }

    fn claim_rescan(&self, min_retry_age: Duration) -> bool {
        if !self.needs_rescan.load(Ordering::Acquire) {
            return false;
        }
        let retry_ready = self
            .rescan_failed_at
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_none_or(|failed_at| failed_at.elapsed() >= min_retry_age);
        retry_ready && self.needs_rescan.swap(false, Ordering::AcqRel)
    }

    pub(super) async fn run_cycle(&self) {
        self.run_cycle_with_retry_age(EXTRACTOR_REPAIR_RETRY_INTERVAL)
            .await;
    }

    pub(super) async fn run_cycle_with_retry_age(&self, min_repair_age: Duration) {
        let reconcile = self.claim_rescan(min_repair_age);
        if reconcile {
            self.indexing_runtime
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .set_watcher_rescan_pending(true);
        }
        self.process_queue_batch().await;
        if reconcile {
            self.reconcile_workspace().await;
        }
    }

    pub(super) async fn process_pending_changes(&self) -> Result<()> {
        self.run_cycle_with_retry_age(Duration::ZERO).await;
        Ok(())
    }

    pub(super) async fn drain_for_shutdown(&self) {
        self.drain_for_shutdown_inner().await;
    }
}

#[cfg(test)]
impl IncrementalIndexer {
    pub(crate) async fn process_pending_changes_with_commit_failure_for_test(&self) -> Result<()> {
        let mut runtime = QueueRuntime::from_indexer(self);
        runtime.fail_commit_for_test = true;
        runtime.process_pending_changes().await
    }
}
