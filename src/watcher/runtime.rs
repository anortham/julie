use super::{FileChangeEvent, FileChangeType, IncrementalIndexer, SharedEmbeddingProvider};
use crate::database::SymbolDatabase;
use crate::extractors::ExtractorManager;
use crate::tools::workspace::indexing::state::{
    IndexingOperation, IndexingRepairReason, SharedIndexingRuntime,
};
use crate::workspace::mutation_gate::acquire_gate;
use anyhow::Result;
use ignore::gitignore::Gitignore;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, info, warn};

const EXTRACTOR_REPAIR_RETRY_INTERVAL: Duration = Duration::from_secs(30);
const DUPLICATE_DEBOUNCE_WINDOW: Duration = Duration::from_secs(1);

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
    needs_rescan: Arc<AtomicBool>,
    tantivy_dirty: Arc<StdMutex<HashSet<String>>>,
    indexing_runtime: SharedIndexingRuntime,
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
            needs_rescan: Arc::clone(&indexer.needs_rescan),
            tantivy_dirty: Arc::clone(&indexer.tantivy_dirty),
            indexing_runtime: Arc::clone(&indexer.indexing_runtime),
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
        needs_rescan: Arc<AtomicBool>,
        tantivy_dirty: Arc<StdMutex<HashSet<String>>>,
        indexing_runtime: SharedIndexingRuntime,
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
            needs_rescan,
            tantivy_dirty,
            indexing_runtime,
        }
    }

    pub(super) async fn run_cycle(&self) {
        self.run_cycle_with_retry_age(EXTRACTOR_REPAIR_RETRY_INTERVAL)
            .await;
    }

    async fn run_cycle_with_retry_age(&self, min_repair_age: Duration) {
        self.retry_dirty_tantivy().await;

        let processed_count = self.process_queue_batch().await;
        if processed_count > 0 {
            self.commit_search_index("batch").await;
        }

        let replayed_repairs = self.retry_persisted_repairs(min_repair_age).await;
        if replayed_repairs > 0 {
            self.commit_search_index("repair replay").await;
        }

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
                let _guard = acquire_gate(&self.workspace_id).await;
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
                        &_guard,
                    )
                    .await;
                }
            } // _guard dropped here, gate released

        }

        self.retry_dirty_tantivy().await;
        self.commit_search_index("shutdown drain").await;
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
        let _guard = acquire_gate(&self.workspace_id).await;

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
                &_guard,
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
        let _guard = acquire_gate(&self.workspace_id).await;

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
            let (symbols, file_doc) = {
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
                let file_doc = crate::search::FileDocument {
                    file_path: rel_path.clone(),
                    content,
                    language,
                };
                (symbols, file_doc)
            };

            let search_index = Arc::clone(search_index);
            let rel_clone = rel_path.clone();
            let retry_result = tokio::task::spawn_blocking(move || {
                let idx = search_index
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                crate::search::projection::apply_uncommitted_documents_from_symbols(
                    &idx,
                    &symbols,
                    std::slice::from_ref(&file_doc),
                    std::slice::from_ref(&rel_clone),
                )?;
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
                    self.indexing_runtime
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .set_dirty_projection_count(remaining_dirty);
                    info!("Tantivy retry succeeded for {}", rel_path);
                }
                Ok(Err(err)) => {
                    warn!("Tantivy retry failed for {}: {}", rel_path, err);
                }
                Err(err) => {
                    warn!("Tantivy retry task panicked for {}: {}", rel_path, err);
                }
            }
        }

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish_operation();
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
        let _guard = acquire_gate(&self.workspace_id).await;

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
                &_guard,
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
        let _guard = acquire_gate(&self.workspace_id).await;

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
                    &_guard,
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
                        &_guard,
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
                &_guard,
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
