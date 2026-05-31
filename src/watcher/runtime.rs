use super::{FileChangeEvent, FileChangeType, IncrementalIndexer, SharedEmbeddingProvider};
use crate::database::SymbolDatabase;
use crate::extractors::ExtractorManager;
use crate::tools::workspace::indexing::state::{
    IndexingOperation, IndexingRepairReason, SharedIndexingRuntime,
};
use crate::watcher::observability::timed_acquire_gate_with_registry_or_cancelled;
use crate::workspace::mutation_gate::{MutationGuard, Registry as MutationGateRegistry};
use anyhow::Result;
use ignore::gitignore::Gitignore;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, error, info, warn};

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
    search_index: Option<Arc<StdMutex<crate::search::SearchIndex>>>,
    embedding_provider: SharedEmbeddingProvider,
    lang_configs: Arc<crate::search::language_config::LanguageConfigs>,
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
        }
    }

    pub(super) fn new(
        db: Arc<StdMutex<SymbolDatabase>>,
        extractor_manager: Arc<ExtractorManager>,
        search_index: Option<Arc<StdMutex<crate::search::SearchIndex>>>,
        embedding_provider: SharedEmbeddingProvider,
        lang_configs: Arc<crate::search::language_config::LanguageConfigs>,
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
        let remaining = self.index_queue.lock().await.len();
        if remaining > 0 {
            info!(
                "Queue processor shutting down, draining {} remaining events",
                remaining
            );
            // Acquire the gate for the dispatch loop, then drop it before
            // calling retry_dirty_tantivy (which acquires its own gate).
            // Holding both simultaneously would deadlock on the same workspace_id.
            {
                let Some(guard) = self.acquire_gate_or_mark_rescan("shutdown drain").await else {
                    return;
                };
                let mut drained_any = false;
                while let Some(event) = self.index_queue.lock().await.pop_front() {
                    let provider_snapshot = self
                        .embedding_provider
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .clone();
                    super::dispatch_file_event(
                        event,
                        &self.db,
                        &self.extractor_manager,
                        &self.search_index,
                        &provider_snapshot,
                        &self.workspace_root,
                        &self.lang_configs,
                        &self.tantivy_dirty,
                        &self.indexing_runtime,
                        &guard,
                    )
                    .await;
                    drained_any = true;
                }
                if drained_any {
                    self.commit_search_index("shutdown drain").await;
                }
            } // guard dropped here, gate released
        }

        self.retry_dirty_tantivy().await;
    }

    async fn retry_persisted_repairs(&self, min_repair_age: Duration) -> usize {
        if !self.index_queue.lock().await.is_empty() {
            return 0;
        }

        let repairs = {
            let db_guard = self
                .db
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            db_guard.list_indexing_repairs().unwrap_or_default()
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(i64::MAX);
        let due_repairs: Vec<_> = repairs
            .into_iter()
            .filter(|repair| {
                repair.reason == IndexingRepairReason::ExtractorFailure.as_str()
                    && now.saturating_sub(repair.updated_at) >= min_repair_age.as_secs() as i64
            })
            .collect();

        if due_repairs.is_empty() {
            return 0;
        }

        // Acquire the mutation gate before dispatching repair events.
        let Some(guard) = self
            .acquire_gate_or_mark_rescan("persisted repair retry")
            .await
        else {
            return 0;
        };

        let gitignore = match super::filtering::build_gitignore_matcher(&self.workspace_root) {
            Ok(gitignore) => Some(gitignore),
            Err(err) => {
                warn!(
                    "Repair retry failed to build gitignore matcher for {}: {}",
                    self.workspace_root.display(),
                    err
                );
                None
            }
        };

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record_repair_reason(IndexingRepairReason::ExtractorFailure);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .begin_operation(IndexingOperation::WatcherRepair);

        let provider_snapshot = self
            .embedding_provider
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        let mut replayed = 0usize;
        for repair in due_repairs {
            let repair_path = repair.path.clone();
            let absolute_path = self.workspace_root.join(&repair_path);
            if !absolute_path.is_file() {
                let db_guard = self
                    .db
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Err(err) = db_guard.clear_indexing_repair(&repair_path) {
                    warn!(
                        "Failed to clear stale persisted repair for {}: {}",
                        repair_path, err
                    );
                }
                continue;
            }

            if !self.repair_path_is_retryable(&absolute_path, gitignore.as_ref()) {
                info!(
                    "Clearing repair for file unsupported by watcher extraction: {}",
                    repair_path
                );
                let db_guard = self
                    .db
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Err(err) = db_guard.clear_indexing_repair(&repair_path) {
                    warn!(
                        "Failed to clear repair for unsupported file {}: {}",
                        repair_path, err
                    );
                }
                continue;
            }

            super::dispatch_file_event(
                FileChangeEvent {
                    path: absolute_path,
                    change_type: FileChangeType::Modified,
                    timestamp: SystemTime::now(),
                },
                &self.db,
                &self.extractor_manager,
                &self.search_index,
                &provider_snapshot,
                &self.workspace_root,
                &self.lang_configs,
                &self.tantivy_dirty,
                &self.indexing_runtime,
                &guard,
            )
            .await;
            replayed += 1;
        }

        let remaining_extractor_repairs = {
            let db_guard = self
                .db
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            db_guard
                .list_indexing_repairs()
                .unwrap_or_default()
                .into_iter()
                .filter(|repair| repair.reason == IndexingRepairReason::ExtractorFailure.as_str())
                .count()
        };

        {
            let mut runtime = self
                .indexing_runtime
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if remaining_extractor_repairs == 0 {
                runtime.clear_repair_reason(IndexingRepairReason::ExtractorFailure);
            } else {
                runtime.record_repair_reason(IndexingRepairReason::ExtractorFailure);
            }
            runtime.finish_operation();
        }

        if replayed > 0 {
            self.commit_search_index("repair replay").await;
        }

        replayed
    }

    fn repair_path_is_retryable(
        &self,
        absolute_path: &Path,
        gitignore: Option<&Gitignore>,
    ) -> bool {
        if !Self::path_has_registered_extractor(absolute_path) {
            return false;
        }

        let Some(gitignore) = gitignore else {
            return true;
        };

        super::filtering::should_index_file(
            absolute_path,
            &self.supported_extensions,
            gitignore,
            &self.workspace_root,
        )
    }

    fn path_has_registered_extractor(path: &Path) -> bool {
        let Some(language) = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(crate::extractors::language::detect_language_from_extension)
        else {
            return false;
        };

        crate::extractors::registry::registry_entry(language).is_ok()
    }

    async fn retry_dirty_tantivy(&self) {
        let dirty_paths: Vec<String> = {
            let dirty = self
                .tantivy_dirty
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            dirty.iter().cloned().collect()
        };

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_dirty_projection_count(dirty_paths.len());

        if dirty_paths.is_empty() {
            return;
        }

        let Some(search_index) = self.search_index.as_ref() else {
            warn!(
                reason = %IndexingRepairReason::TantivyDirty,
                dirty_files = dirty_paths.len(),
                "Skipping dirty Tantivy retry because no search index is attached"
            );
            return;
        };

        // Acquire the mutation gate before writing to Tantivy.
        let Some(_guard) = self
            .acquire_gate_or_mark_rescan("dirty Tantivy retry")
            .await
        else {
            return;
        };

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .begin_operation(IndexingOperation::WatcherRepair);

        warn!(
            reason = %IndexingRepairReason::TantivyDirty,
            dirty_files = dirty_paths.len(),
            "Retrying dirty Tantivy projection entries"
        );

        for rel_path in dirty_paths {
            let (symbols, file_content, file_language) = {
                let db_guard = self
                    .db
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let symbols = db_guard.get_symbols_for_file(&rel_path).unwrap_or_default();
                let content = db_guard
                    .get_file_content(&rel_path)
                    .unwrap_or(None)
                    .unwrap_or_default();
                let language = symbols
                    .first()
                    .map(|symbol| symbol.language.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                (symbols, content, language)
            };

            let search_index = Arc::clone(search_index);
            let db_for_retry = Arc::clone(&self.db);
            let rel_clone = rel_path.clone();
            let retry_result = tokio::task::spawn_blocking(move || {
                let idx = search_index
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let db_guard = db_for_retry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let symbol_ids: Vec<String> =
                    symbols.iter().map(|symbol| symbol.id.clone()).collect();
                let partner_symbol_ids =
                    crate::search::projection::collect_relationship_partner_symbol_ids(
                        &db_guard,
                        &symbol_ids,
                    )?;
                crate::search::projection::apply_uncommitted_documents_from_symbols(
                    &idx,
                    &symbols,
                    &rel_clone,
                    &file_content,
                    &file_language,
                    std::slice::from_ref(&rel_clone),
                    &db_guard,
                )?;
                if !partner_symbol_ids.is_empty() {
                    crate::search::projection::reproject_partner_symbols(
                        &idx,
                        &db_guard,
                        &partner_symbol_ids,
                    )?;
                }
                idx.commit()?;
                Ok::<(), anyhow::Error>(())
            })
            .await;

            match retry_result {
                Ok(Ok(())) => {
                    let remaining_dirty = {
                        let mut dirty = self
                            .tantivy_dirty
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        dirty.remove(&rel_path);
                        dirty.len()
                    };
                    self.tantivy_failure_attempts
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .remove(&rel_path);
                    {
                        let mut runtime = self
                            .indexing_runtime
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        runtime.set_dirty_projection_count(remaining_dirty);
                        runtime.clear_abandoned_projection(&rel_path);
                    }
                    info!("Tantivy retry succeeded for {}", rel_path);
                }
                Ok(Err(err)) => {
                    self.handle_tantivy_retry_failure(&rel_path, &err.to_string());
                }
                Err(err) => {
                    self.handle_tantivy_retry_failure(
                        &rel_path,
                        &format!("retry task panicked: {}", err),
                    );
                }
            }
        }

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish_operation();
    }

    /// Bumps the per-file retry counter and decides whether to keep retrying,
    /// log a warning, or abandon the file entirely. After
    /// MAX_TANTIVY_RETRY_ATTEMPTS we drop the file from the dirty set, emit a
    /// single ERROR with remediation guidance, and record a repair reason so
    /// the health report surfaces the projection failure instead of letting it
    /// hide behind a silent retry loop.
    fn handle_tantivy_retry_failure(&self, rel_path: &str, error_text: &str) {
        let attempts = {
            let mut attempts = self
                .tantivy_failure_attempts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let count = attempts.entry(rel_path.to_string()).or_insert(0);
            *count += 1;
            *count
        };

        if attempts == 1 {
            warn!(
                "Tantivy retry failed for {} (will retry up to {} times): {}",
                rel_path, MAX_TANTIVY_RETRY_ATTEMPTS, error_text
            );
        } else if attempts >= MAX_TANTIVY_RETRY_ATTEMPTS {
            let remaining_dirty = {
                let mut dirty = self
                    .tantivy_dirty
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                dirty.remove(rel_path);
                dirty.len()
            };
            self.tantivy_failure_attempts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(rel_path);

            let mut runtime = self
                .indexing_runtime
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            runtime.set_dirty_projection_count(remaining_dirty);
            runtime.record_abandoned_projection(rel_path.to_string());
            drop(runtime);

            error!(
                file = %rel_path,
                attempts = attempts,
                last_error = %error_text,
                "Tantivy projection abandoned for {} after {} retries — index directory may be missing on disk. Run manage_workspace operation=index force=true to rebuild.",
                rel_path,
                MAX_TANTIVY_RETRY_ATTEMPTS
            );
        }
    }

    async fn process_queue_batch(&self) -> usize {
        let queue_size = {
            let queue = self.index_queue.lock().await;
            queue.len()
        };

        if queue_size == 0 {
            return 0;
        }

        debug!("Processing {} queued file events", queue_size);

        // Acquire the mutation gate for the duration of the batch.  Held until
        // all events in this tick are dispatched so catch-up indexing cannot
        // interleave writes mid-batch.
        let Some(guard) = self.acquire_gate_or_mark_rescan("queue batch").await else {
            return 0;
        };

        let mut processed_count = 0usize;
        let mut dropped_duplicates = 0usize;
        let mut deletes = 0usize;
        let mut renames = 0usize;
        let max_this_tick = queue_size;
        let mut iterations = 0usize;

        while iterations < max_this_tick {
            let event = match {
                let mut queue = self.index_queue.lock().await;
                queue.pop_front()
            } {
                Some(event) => event,
                None => break,
            };
            iterations += 1;

            let should_drop_duplicate = {
                let mut last_processed = self.last_processed.lock().await;
                let now = SystemTime::now();

                match event.change_type {
                    FileChangeType::Created | FileChangeType::Modified => {
                        if let Some(last_time) = last_processed.get(&event.path) {
                            if let Ok(elapsed) = now.duration_since(*last_time) {
                                if elapsed < DUPLICATE_DEBOUNCE_WINDOW {
                                    debug!(
                                        "Dropping duplicate event for {:?} (processed {}ms ago)",
                                        event.path,
                                        elapsed.as_millis()
                                    );
                                    true
                                } else {
                                    last_processed.insert(event.path.clone(), now);
                                    false
                                }
                            } else {
                                last_processed.insert(event.path.clone(), now);
                                false
                            }
                        } else {
                            last_processed.insert(event.path.clone(), now);
                            false
                        }
                    }
                    FileChangeType::Deleted | FileChangeType::Renamed { .. } => {
                        last_processed.insert(event.path.clone(), now);
                        false
                    }
                }
            };

            if should_drop_duplicate {
                dropped_duplicates += 1;
                continue;
            }

            match event.change_type {
                FileChangeType::Deleted => deletes += 1,
                FileChangeType::Renamed { .. } => renames += 1,
                FileChangeType::Created | FileChangeType::Modified => {}
            }

            debug!("Background task processing: {:?}", event.path);

            let provider_snapshot = self
                .embedding_provider
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();

            let atomic_delete_path = super::dispatch_file_event(
                event,
                &self.db,
                &self.extractor_manager,
                &self.search_index,
                &provider_snapshot,
                &self.workspace_root,
                &self.lang_configs,
                &self.tantivy_dirty,
                &self.indexing_runtime,
                &guard,
            )
            .await;

            if let Some(path) = atomic_delete_path {
                self.last_processed.lock().await.remove(&path);
            }

            processed_count += 1;
        }

        let remaining_queue_len = self.index_queue.lock().await.len();
        if processed_count > 0
            || dropped_duplicates > 0
            || deletes > 0
            || renames > 0
            || remaining_queue_len > 0
        {
            info!(
                processed = processed_count,
                dropped_duplicates, deletes, renames, remaining_queue_len, "Watcher batch summary"
            );
        }

        {
            let mut last_processed = self.last_processed.lock().await;
            last_processed.retain(|_, timestamp| {
                timestamp
                    .elapsed()
                    .map(|elapsed| elapsed < Duration::from_secs(2))
                    .unwrap_or(false)
            });
        }

        if processed_count > 0 {
            self.commit_search_index("batch").await;
        }

        processed_count
    }

    async fn run_repair_scan_if_needed(&self) {
        let queue_now_empty = self.index_queue.lock().await.is_empty();
        let rescan_pending = self.needs_rescan.load(Ordering::Acquire);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_watcher_rescan_pending(rescan_pending);

        if !queue_now_empty || !rescan_pending {
            return;
        }

        // Acquire the mutation gate after early-return checks pass.
        let Some(guard) = self.acquire_gate_or_mark_rescan("repair scan").await else {
            return;
        };

        self.needs_rescan.store(false, Ordering::Release);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_watcher_rescan_pending(false);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .begin_operation(IndexingOperation::WatcherRepair);
        warn!(
            reason = %IndexingRepairReason::WatcherOverflow,
            "Queue overflow detected, running repair scan for stale and new files"
        );

        let repair_started = Instant::now();
        let indexed_hashes = {
            let db_guard = self
                .db
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            db_guard.get_file_hashes_for_workspace().unwrap_or_default()
        };
        let indexed_set: HashSet<String> = indexed_hashes.keys().cloned().collect();

        let workspace_files = match crate::startup::scan_workspace_files(&self.workspace_root) {
            Ok(files) => files,
            Err(err) => {
                warn!(
                    "Repair scan failed to enumerate workspace files for {}: {}",
                    self.workspace_root.display(),
                    err
                );
                self.needs_rescan.store(true, Ordering::Release);
                self.indexing_runtime
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .set_watcher_rescan_pending(true);
                self.indexing_runtime
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .finish_operation();
                return;
            }
        };
        let gitignore = match super::filtering::build_gitignore_matcher(&self.workspace_root) {
            Ok(gitignore) => gitignore,
            Err(err) => {
                warn!(
                    "Repair scan failed to build gitignore matcher for {}: {}",
                    self.workspace_root.display(),
                    err
                );
                self.needs_rescan.store(true, Ordering::Release);
                self.indexing_runtime
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .set_watcher_rescan_pending(true);
                self.indexing_runtime
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .finish_operation();
                return;
            }
        };

        let provider_snapshot = self
            .embedding_provider
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        let mut checked_indexed_files = 0usize;
        let mut skipped_unchanged_files = 0usize;
        let mut deleted_files = 0usize;
        let mut modified_files = 0usize;
        let mut new_files = 0usize;
        let mut failed_hash_reads = 0usize;
        let mut dispatched_events = 0usize;

        for (rel_path, stored_hash) in &indexed_hashes {
            checked_indexed_files += 1;
            let abs_path = self.workspace_root.join(std::path::Path::new(rel_path));
            if !abs_path.is_file() {
                super::dispatch_file_event(
                    FileChangeEvent {
                        path: abs_path,
                        change_type: FileChangeType::Deleted,
                        timestamp: SystemTime::now(),
                    },
                    &self.db,
                    &self.extractor_manager,
                    &self.search_index,
                    &provider_snapshot,
                    &self.workspace_root,
                    &self.lang_configs,
                    &self.tantivy_dirty,
                    &self.indexing_runtime,
                    &guard,
                )
                .await;
                deleted_files += 1;
                dispatched_events += 1;
                continue;
            }

            match crate::database::calculate_file_hash(&abs_path) {
                Ok(current_hash) if current_hash != *stored_hash => {
                    super::dispatch_file_event(
                        FileChangeEvent {
                            path: abs_path,
                            change_type: FileChangeType::Modified,
                            timestamp: SystemTime::now(),
                        },
                        &self.db,
                        &self.extractor_manager,
                        &self.search_index,
                        &provider_snapshot,
                        &self.workspace_root,
                        &self.lang_configs,
                        &self.tantivy_dirty,
                        &self.indexing_runtime,
                        &guard,
                    )
                    .await;
                    modified_files += 1;
                    dispatched_events += 1;
                }
                Ok(_) => {
                    skipped_unchanged_files += 1;
                }
                Err(err) => {
                    failed_hash_reads += 1;
                    warn!(
                        "Repair scan hash read failed for {}: {}",
                        abs_path.display(),
                        err
                    );
                }
            }
        }

        for rel_path in workspace_files.difference(&indexed_set) {
            let abs_path = self.workspace_root.join(std::path::Path::new(rel_path));
            if !super::filtering::should_index_file(
                &abs_path,
                &self.supported_extensions,
                &gitignore,
                &self.workspace_root,
            ) {
                continue;
            }

            super::dispatch_file_event(
                FileChangeEvent {
                    path: abs_path,
                    change_type: FileChangeType::Created,
                    timestamp: SystemTime::now(),
                },
                &self.db,
                &self.extractor_manager,
                &self.search_index,
                &provider_snapshot,
                &self.workspace_root,
                &self.lang_configs,
                &self.tantivy_dirty,
                &self.indexing_runtime,
                &guard,
            )
            .await;
            new_files += 1;
            dispatched_events += 1;
        }

        info!(
            checked_indexed_files,
            skipped_unchanged_files,
            deleted_files,
            modified_files,
            new_files,
            failed_hash_reads,
            elapsed_ms = repair_started.elapsed().as_millis(),
            "Post-overflow repair scan summary"
        );

        if dispatched_events > 0 {
            self.commit_search_index("repair scan").await;
        }
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish_operation();
    }

    async fn commit_search_index(&self, context: &str) {
        let Some(search_index) = self.search_index.as_ref() else {
            return;
        };

        let search_index = Arc::clone(search_index);
        let context = context.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            let idx = search_index
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Err(err) = idx.commit() {
                warn!("Failed to commit Tantivy {}: {}", context, err);
            }
        })
        .await;
    }
}
