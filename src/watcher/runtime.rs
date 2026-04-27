use super::{FileChangeEvent, FileChangeType, IncrementalIndexer, SharedEmbeddingProvider};
use crate::database::SymbolDatabase;
use crate::extractors::ExtractorManager;
use crate::tools::workspace::indexing::state::{
    IndexingOperation, IndexingRepairReason, SharedIndexingRuntime,
};
use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, info, warn};

const EXTRACTOR_REPAIR_RETRY_INTERVAL: Duration = Duration::from_secs(30);

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
    pause_flag: Arc<AtomicBool>,
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
            pause_flag: Arc::clone(&indexer.pause_flag),
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
        pause_flag: Arc<AtomicBool>,
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
            pause_flag,
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
        if self.pause_flag.load(Ordering::Acquire) {
            self.indexing_runtime
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .set_watcher_paused(true);
            return;
        }

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_watcher_paused(false);

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
                )
                .await;
            }
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

            // Skip files with unsupported extensions (e.g., binary media files
            // that leaked into the repair table from earlier indexing runs).
            // Extensionless files are allowed through — they may be valid
            // targets like Dockerfile or Makefile.
            let has_unsupported_ext = absolute_path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| !self.supported_extensions.contains(ext));
            if has_unsupported_ext {
                info!(
                    "Clearing repair for unsupported file type: {}",
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

        if queue_size > 0 {
            debug!("Processing {} queued file events", queue_size);
        }

        let mut processed_count = 0usize;
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

            let should_skip = {
                let mut last_processed = self.last_processed.lock().await;
                let now = SystemTime::now();

                if let Some(last_time) = last_processed.get(&event.path) {
                    if let Ok(elapsed) = now.duration_since(*last_time) {
                        if elapsed < Duration::from_secs(1) {
                            debug!(
                                "Skipping duplicate event for {:?} (processed {}ms ago)",
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
            };

            if should_skip {
                let mut queue = self.index_queue.lock().await;
                queue.push_back(event);
                continue;
            }

            info!("Background task processing: {:?}", event.path);

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
            )
            .await;

            if let Some(path) = atomic_delete_path {
                self.last_processed.lock().await.remove(&path);
            }

            processed_count += 1;
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

        let indexed_files = {
            let db_guard = self
                .db
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            db_guard.get_all_indexed_files().unwrap_or_default()
        };
        let indexed_set: HashSet<String> = indexed_files.iter().cloned().collect();

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
                return;
            }
        };

        let provider_snapshot = self
            .embedding_provider
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        for rel_path in &indexed_files {
            let abs_path = self.workspace_root.join(std::path::Path::new(rel_path));
            let change_type = if abs_path.is_file() {
                FileChangeType::Modified
            } else {
                FileChangeType::Deleted
            };
            let repair_event = FileChangeEvent {
                path: abs_path,
                change_type,
                timestamp: SystemTime::now(),
            };
            super::dispatch_file_event(
                repair_event,
                &self.db,
                &self.extractor_manager,
                &self.search_index,
                &provider_snapshot,
                &self.workspace_root,
                &self.lang_configs,
                &self.tantivy_dirty,
                &self.indexing_runtime,
            )
            .await;
        }

        let mut new_file_count = 0usize;
        for rel_path in workspace_files.difference(&indexed_set) {
            let abs_path = self.workspace_root.join(std::path::Path::new(rel_path));
            if !abs_path.is_file() {
                continue;
            }

            if let Some(ext) = abs_path.extension().and_then(|ext| ext.to_str()) {
                if !self.supported_extensions.contains(ext) {
                    continue;
                }
            }

            let repair_event = FileChangeEvent {
                path: abs_path,
                change_type: FileChangeType::Created,
                timestamp: SystemTime::now(),
            };
            super::dispatch_file_event(
                repair_event,
                &self.db,
                &self.extractor_manager,
                &self.search_index,
                &provider_snapshot,
                &self.workspace_root,
                &self.lang_configs,
                &self.tantivy_dirty,
                &self.indexing_runtime,
            )
            .await;
            new_file_count += 1;
        }

        info!(
            "Post-overflow repair scan: checked {} indexed files, discovered {} new files",
            indexed_files.len(),
            new_file_count
        );

        self.commit_search_index("repair scan").await;
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
