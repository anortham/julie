use super::{FileChangeEvent, FileChangeType, IncrementalIndexer, SharedEmbeddingProvider};
use crate::watcher::observability::timed_acquire_gate_with_registry_or_cancelled;
use crate::workspace::mutation_gate::{MutationGuard, Registry as MutationGateRegistry};
use anyhow::Result;
use ignore::gitignore::Gitignore;
use julie_core::database::{ProjectionStatus, SymbolDatabase};
use julie_core::indexing_state::{IndexingOperation, IndexingRepairReason, SharedIndexingRuntime};
use julie_extractors::ExtractorManager;
use julie_index::search::projection::TANTIVY_PROJECTION_NAME;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, error, info, warn};

mod processing;
mod projection;
mod repairs;

const EXTRACTOR_REPAIR_RETRY_INTERVAL: Duration = Duration::from_secs(30);
const DUPLICATE_DEBOUNCE_WINDOW: Duration = Duration::from_secs(1);
/// Maximum number of times a single file's Tantivy projection retry can fail
/// before we abandon retrying it. With a 1-second retry tick this means we
/// stop after ~10 seconds, which is long enough to ride out transient
/// filesystem hiccups but short enough to stop log spam when the index
/// directory has been deleted out from under the daemon.
const MAX_TANTIVY_RETRY_ATTEMPTS: u32 = 10;

#[derive(Clone)]
pub(super) struct QueueRuntime {
    db: Arc<StdMutex<SymbolDatabase>>,
    extractor_manager: Arc<ExtractorManager>,
    search_index: Option<Arc<julie_index::search::SearchIndex>>,
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
}

impl QueueRuntime {
    pub(super) fn from_indexer(indexer: &IncrementalIndexer) -> Self {
        Self {
            db: Arc::clone(&indexer.db),
            extractor_manager: Arc::clone(&indexer.extractor_manager),
            search_index: indexer.search_index.as_ref().map(Arc::clone),
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
        }
    }

    pub(super) fn new(
        db: Arc<StdMutex<SymbolDatabase>>,
        extractor_manager: Arc<ExtractorManager>,
        search_index: Option<Arc<julie_index::search::SearchIndex>>,
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
            db,
            extractor_manager,
            search_index,
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
        }
    }

    async fn acquire_gate_or_mark_rescan(&self, context: &str) -> Option<MutationGuard<'static>> {
        let guard = timed_acquire_gate_with_registry_or_cancelled(
            &self.mutation_gate_registry,
            &self.workspace_id,
            Duration::from_millis(100),
            &self.cancel_flag,
        )
        .await;

        if guard.is_none() {
            self.mark_rescan_pending_due_to_cancelled_gate(context);
        }

        guard
    }

    fn mark_rescan_pending_due_to_cancelled_gate(&self, context: &str) {
        self.needs_rescan.store(true, Ordering::Release);
        let mut runtime = self
            .indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.set_watcher_rescan_pending(true);
        runtime.record_repair_reason(IndexingRepairReason::WatcherOverflow);
        warn!(
            workspace_id = %self.workspace_id,
            context,
            "Watcher shutdown skipped queued mutation because the mutation gate was held; rescan marked pending"
        );
    }

    pub(super) async fn run_cycle(&self) {
        self.run_cycle_with_retry_age(EXTRACTOR_REPAIR_RETRY_INTERVAL)
            .await;
    }

    async fn run_cycle_with_retry_age(&self, min_repair_age: Duration) {
        self.retry_dirty_tantivy().await;

        self.process_queue_batch().await;

        self.retry_persisted_repairs(min_repair_age).await;

        self.run_repair_scan_if_needed().await;
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
