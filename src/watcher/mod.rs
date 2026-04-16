//! File Watcher & Incremental Indexing System
//!
//! This module provides real-time file monitoring and incremental updates
//! to the SQLite database and Tantivy search index.
//!
//! # Architecture
//!
//! The watcher uses a 2-phase processing model:
//! 1. **File System Events** -> Notify-rs detects changes and queues them
//! 2. **Background Processing** -> Async task processes queue every second
//!
//! This separation prevents blocking on file I/O or database operations.

pub(crate) mod events;
pub mod filtering; // Public for tests
pub mod handlers; // Public for tests
mod runtime;
pub mod types;

use anyhow::{Context, Result};
use ignore::gitignore::Gitignore;
use notify::Watcher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::SystemTime;
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tracing::{debug, error, info, warn};

use crate::database::SymbolDatabase;
use crate::extractors::ExtractorManager;
use crate::tools::workspace::indexing::state::{IndexingRepairReason, SharedIndexingRuntime};

pub use types::{FileChangeEvent, FileChangeType, IndexingStats};

/// Shared embedding provider that can be updated after construction.
/// The workspace and watcher hold clones of the same Arc, so when
/// `initialize_embedding_provider()` writes a new provider, the watcher's
/// background tasks see it on their next read-lock.
pub(crate) type SharedEmbeddingProvider =
    Arc<std::sync::RwLock<Option<Arc<dyn crate::embeddings::EmbeddingProvider>>>>;

/// Manages incremental indexing with real-time file watching
pub struct IncrementalIndexer {
    watcher: Option<notify::RecommendedWatcher>,
    db: Arc<StdMutex<SymbolDatabase>>,
    extractor_manager: Arc<ExtractorManager>,
    search_index: Option<Arc<StdMutex<crate::search::SearchIndex>>>,

    /// Embedding provider for incremental semantic updates.
    /// Shared with the workspace via Arc<RwLock<...>> so lazy initialization
    /// (which happens on first search) propagates to the watcher.
    embedding_provider: SharedEmbeddingProvider,

    /// Language configs for embedding text generation (extra kinds per language).
    lang_configs: Arc<crate::search::language_config::LanguageConfigs>,

    // Processing queues
    pub(crate) index_queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>>,

    // Event deduplication: Track recently processed files to avoid duplicate processing
    // Key: file path, Value: last processed timestamp
    last_processed: Arc<TokioMutex<HashMap<PathBuf, SystemTime>>>,

    // File filters
    supported_extensions: HashSet<String>,
    gitignore: Gitignore,

    // Configuration
    workspace_root: PathBuf,

    /// Shared flag checked by spawned tasks — when set to true, tasks exit their loops.
    cancel_flag: Arc<AtomicBool>,

    /// When true, the queue processor skips dispatching events (events remain buffered).
    /// Used to pause the watcher during catch-up indexing to prevent concurrent updates.
    pub(crate) pause_flag: Arc<AtomicBool>,

    /// Set to true when the event queue overflows (>1000 events).
    /// The queue processor and external callers check this to trigger a full rescan.
    pub(crate) needs_rescan: Arc<AtomicBool>,

    /// Relative paths of files whose SQLite update succeeded but Tantivy update
    /// failed. Retried at the start of each queue-processor tick (Fix B-b).
    tantivy_dirty: Arc<StdMutex<std::collections::HashSet<String>>>,

    /// Shared indexing runtime snapshot for health and dashboard reporting.
    indexing_runtime: SharedIndexingRuntime,

    /// Join handles for the event detector and queue processor tasks.
    /// Stored so stop() can join them for a clean, non-aborting shutdown (Fix D).
    event_task: Option<tokio::task::JoinHandle<()>>,
    queue_task: Option<tokio::task::JoinHandle<()>>,
}

/// Dispatch a single file change event to the appropriate handler.
///
/// Returns `Some(path)` when a DELETE event is skipped because the file still
/// exists (atomic-save pattern). The caller should remove that path from its
/// dedup map so the follow-up Create/Modify event is not suppressed (Fix F:
/// replaces the old detached `tokio::spawn` callback approach).
pub(super) async fn dispatch_file_event(
    event: FileChangeEvent,
    db: &Arc<StdMutex<SymbolDatabase>>,
    extractor_manager: &Arc<ExtractorManager>,
    search_index: &Option<Arc<StdMutex<crate::search::SearchIndex>>>,
    embedding_provider: &Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
    workspace_root: &std::path::Path,
    lang_configs: &Arc<crate::search::language_config::LanguageConfigs>,
    tantivy_dirty: &Arc<StdMutex<std::collections::HashSet<String>>>,
    indexing_runtime: &SharedIndexingRuntime,
) -> Option<PathBuf> {
    let relative_for_embed =
        crate::utils::paths::to_relative_unix_style(&event.path, workspace_root).ok();

    match event.change_type {
        FileChangeType::Created | FileChangeType::Modified => {
            let rel_path = relative_for_embed.clone();
            match handlers::handle_file_created_or_modified_static(
                event.path,
                db,
                extractor_manager,
                workspace_root,
                search_index.as_ref(),
            )
            .await
            {
                Err(e) => {
                    warn!("Failed to handle file change: {}", e);
                }
                Ok(outcome) => {
                    // Fix B-b: track Tantivy failures for retry on next tick
                    if !outcome.tantivy_ok {
                        if let Some(ref rel) = rel_path {
                            let mut dirty = tantivy_dirty.lock().unwrap_or_else(|p| p.into_inner());
                            dirty.insert(rel.clone());
                            warn!("Tantivy update failed for {}; queued for retry", rel);
                        }
                    }
                    if let Some(reason) = outcome.repair_reason {
                        indexing_runtime
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .record_repair_reason(reason);
                        warn!(%reason, "Watcher repair needed after file change");
                    }
                    // Fix E: wrap blocking IPC call in spawn_blocking
                    if let (Some(provider), Some(rel)) = (embedding_provider, &rel_path) {
                        let db_clone = Arc::clone(db);
                        let provider_clone = Arc::clone(provider);
                        let rel_owned = rel.clone();
                        let lc = Arc::clone(lang_configs);
                        if let Err(e) = tokio::task::spawn_blocking(move || {
                            crate::embeddings::pipeline::reembed_symbols_for_file(
                                &db_clone,
                                provider_clone.as_ref(),
                                &rel_owned,
                                Some(lc.as_ref()),
                            )
                        })
                        .await
                        {
                            warn!("Incremental embedding task panicked: {}", e);
                        }
                    }
                }
            }
            None
        }
        FileChangeType::Deleted => {
            // Guard: if the file still exists, this was likely an atomic
            // save (write-temp → delete → rename). Skip to avoid nuking
            // valid data — the subsequent Create/Modify event will re-index.
            if event.path.exists() {
                info!(
                    "Skipping DELETE for {} (file still exists, likely atomic save)",
                    event.path.display()
                );
                // Fix F: return the path for inline dedup-map clearing instead
                // of spawning a detached tokio task (W7).
                return Some(event.path);
            }

            if let Some(ref rel) = relative_for_embed {
                if let Ok(mut db_guard) = db.lock() {
                    if let Err(e) = db_guard.delete_embeddings_for_file(rel) {
                        warn!("Failed to delete embeddings for {}: {}", rel, e);
                    }
                }
            }
            if let Err(e) = handlers::handle_file_deleted_static(
                event.path,
                db,
                workspace_root,
                search_index.as_ref(),
            )
            .await
            {
                warn!("Failed to handle file deletion: {}", e);
            }
            // Clear dirty-retry entry: file is deleted, retrying Tantivy
            // would recreate a phantom doc for a nonexistent file.
            if let Some(ref rel) = relative_for_embed {
                tantivy_dirty
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(rel);
            }
            None
        }
        FileChangeType::Renamed { from, to } => {
            let rel_from = crate::utils::paths::to_relative_unix_style(&from, workspace_root).ok();
            match handlers::handle_file_renamed_static(
                from,
                to.clone(),
                db,
                extractor_manager,
                workspace_root,
                search_index.as_ref(),
            )
            .await
            {
                Err(e) => {
                    indexing_runtime
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .record_repair_reason(IndexingRepairReason::DeletedFiles);
                    warn!("Failed to handle file rename: {}", e);
                }
                Ok(outcome) => {
                    let source_retired =
                        outcome.repair_reason != Some(IndexingRepairReason::ExtractorFailure);
                    if source_retired {
                        if let Some(ref rel_from) = rel_from {
                            if let Ok(mut db_guard) = db.lock() {
                                let _ = db_guard.delete_embeddings_for_file(rel_from);
                            }
                            // Clear old path from dirty-retry set only after the source
                            // has been retired successfully.
                            tantivy_dirty
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .remove(rel_from);
                        }
                    }
                    // Track Tantivy failure on rename's create side for dirty-retry.
                    if !outcome.tantivy_ok {
                        if let Ok(ref rel_to) =
                            crate::utils::paths::to_relative_unix_style(&to, workspace_root)
                        {
                            tantivy_dirty
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .insert(rel_to.clone());
                            warn!(
                                "Tantivy update failed for rename target {}; queued for retry",
                                rel_to
                            );
                        }
                    }
                    if let Some(reason) = outcome.repair_reason {
                        indexing_runtime
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .record_repair_reason(reason);
                        warn!(%reason, "Watcher repair needed after file rename");
                    }
                }
            }
            if let (Some(provider), Ok(rel_to)) = (
                embedding_provider,
                crate::utils::paths::to_relative_unix_style(&to, workspace_root),
            ) {
                // Fix E: wrap blocking IPC call in spawn_blocking
                let db_clone = Arc::clone(db);
                let provider_clone = Arc::clone(provider);
                let rel_owned = rel_to.clone();
                let lc = Arc::clone(lang_configs);
                if let Err(e) = tokio::task::spawn_blocking(move || {
                    crate::embeddings::pipeline::reembed_symbols_for_file(
                        &db_clone,
                        provider_clone.as_ref(),
                        &rel_owned,
                        Some(lc.as_ref()),
                    )
                })
                .await
                {
                    warn!("Incremental embedding task panicked for rename: {}", e);
                }
            }
            None
        }
    }
}

impl IncrementalIndexer {
    /// Create a new incremental indexer for the given workspace
    pub(crate) fn new(
        workspace_root: PathBuf,
        db: Arc<StdMutex<SymbolDatabase>>,
        extractor_manager: Arc<ExtractorManager>,
        search_index: Option<Arc<StdMutex<crate::search::SearchIndex>>>,
        embedding_provider: SharedEmbeddingProvider,
        indexing_runtime: SharedIndexingRuntime,
    ) -> Result<Self> {
        let supported_extensions = filtering::build_supported_extensions();
        let gitignore = filtering::build_gitignore_matcher(&workspace_root)?;
        let lang_configs =
            Arc::new(crate::search::language_config::LanguageConfigs::load_embedded());

        Ok(Self {
            watcher: None,
            db,
            extractor_manager,
            search_index,
            embedding_provider,
            lang_configs,
            index_queue: Arc::new(TokioMutex::new(VecDeque::new())),
            last_processed: Arc::new(TokioMutex::new(HashMap::new())),
            supported_extensions,
            gitignore,
            workspace_root,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            pause_flag: Arc::new(AtomicBool::new(false)),
            needs_rescan: Arc::new(AtomicBool::new(false)),
            tantivy_dirty: Arc::new(StdMutex::new(std::collections::HashSet::new())),
            indexing_runtime,
            event_task: None,
            queue_task: None,
        })
    }

    /// Update the shared embedding provider after lazy initialization.
    pub fn update_embedding_provider(
        &self,
        provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
    ) {
        let mut guard = self
            .embedding_provider
            .write()
            .unwrap_or_else(|p| p.into_inner());
        *guard = provider;
    }

    /// Start watching the workspace for file changes
    pub async fn start_watching(&mut self) -> Result<()> {
        info!(
            "Starting file watcher for workspace: {}",
            self.workspace_root.display()
        );

        let (tx, mut rx) = mpsc::unbounded_channel::<notify::Result<notify::Event>>();

        // Create the watcher
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Err(e) = tx.send(res) {
                error!("Failed to send file event: {}", e);
            }
        })?;

        // Start watching the workspace
        watcher
            .watch(&self.workspace_root, notify::RecursiveMode::Recursive)
            .context("Failed to start watching workspace")?;

        self.watcher = Some(watcher);

        // Start the event processing task
        // Clone the necessary components that are Sync
        let supported_extensions = self.supported_extensions.clone();
        let gitignore = self.gitignore.clone();
        let workspace_root_for_events = self.workspace_root.clone();
        let index_queue = self.index_queue.clone();
        let needs_rescan_for_events = self.needs_rescan.clone();

        let event_handle = tokio::spawn(async move {
            info!("File system event detector started");
            while let Some(event_result) = rx.recv().await {
                match event_result {
                    Ok(event) => {
                        debug!("File system event detected: {:?}", event);
                        if let Err(e) = events::process_file_system_event(
                            &supported_extensions,
                            &gitignore,
                            &workspace_root_for_events,
                            index_queue.clone(),
                            event,
                            &needs_rescan_for_events,
                        )
                        .await
                        {
                            error!("Error processing file system event: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("File watcher error: {}", e);
                    }
                }
            }
        });
        self.event_task = Some(event_handle);

        // Spawn background task to process queued events
        // Clone all the components needed for processing
        let cancel_flag_queue = self.cancel_flag.clone();
        let queue_runtime = runtime::QueueRuntime::new(
            Arc::clone(&self.db),
            Arc::clone(&self.extractor_manager),
            self.search_index.as_ref().map(Arc::clone),
            Arc::clone(&self.embedding_provider),
            Arc::clone(&self.lang_configs),
            Arc::clone(&self.index_queue),
            Arc::clone(&self.last_processed),
            self.supported_extensions.clone(),
            self.workspace_root.clone(),
            Arc::clone(&self.pause_flag),
            Arc::clone(&self.needs_rescan),
            Arc::clone(&self.tantivy_dirty),
            Arc::clone(&self.indexing_runtime),
        );

        let queue_handle = tokio::spawn(async move {
            use tokio::time::{Duration, interval};
            let mut tick = interval(Duration::from_secs(1)); // Process queue every second

            info!("Background queue processor started");
            loop {
                tick.tick().await;

                if cancel_flag_queue.load(Ordering::Acquire) {
                    queue_runtime.drain_for_shutdown().await;
                    info!("Queue processor cancelled, exiting");
                    break;
                }

                queue_runtime.run_cycle().await;
            }
        });
        self.queue_task = Some(queue_handle);

        info!("File watcher started successfully with background queue processing");
        Ok(())
    }

    /// Process any pending file changes from the queue
    pub async fn process_pending_changes(&self) -> Result<()> {
        runtime::QueueRuntime::from_indexer(self)
            .process_pending_changes()
            .await
    }

    /// Stop the file watcher and signal spawned tasks to exit.
    ///
    /// Fix D: Uses cancel flag + join instead of abort(). abort() cuts the task
    /// at the next await point — if that happens after the SQLite commit but before
    /// the Tantivy commit, state is inconsistent. With this approach:
    ///   - Event task: exits when rx yields None (watcher dropped closes the channel).
    ///   - Queue task: checks cancel_flag at the top of each tick; exits within 1s.
    /// Neither task is interrupted mid-event.
    pub async fn stop(&mut self) -> Result<()> {
        // Signal all tasks to stop after their current work item.
        self.cancel_flag.store(true, Ordering::Release);

        // Drop the watcher first — this closes the notify channel sender (tx),
        // causing the event task's rx.recv().await to return None and exit cleanly.
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
        }

        // Join event task (exits quickly once rx yields None).
        if let Some(handle) = self.event_task.take() {
            let _ = handle.await;
        }
        // Join queue task (checks cancel_flag at each tick; exits within ~1s).
        if let Some(handle) = self.queue_task.take() {
            let _ = handle.await;
        }

        info!("File watcher stopped");
        Ok(())
    }

    /// Pause event dispatch. Events continue to accumulate in the queue
    /// but are not dispatched until `resume()` is called.
    ///
    /// Used by `run_auto_indexing` to prevent the watcher from racing with
    /// the catch-up staleness scan (Fix C part a).
    pub fn pause(&self) {
        self.pause_flag.store(true, Ordering::Release);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_watcher_paused(true);
        debug!("File watcher paused");
    }

    /// Resume event dispatch after a `pause()`.
    pub fn resume(&self) {
        self.pause_flag.store(false, Ordering::Release);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_watcher_paused(false);
        debug!("File watcher resumed");
    }

    #[cfg(test)]
    pub fn is_running_for_test(&self) -> bool {
        self.watcher.is_some()
            && self.event_task.is_some()
            && self.queue_task.is_some()
            && !self.cancel_flag.load(Ordering::Acquire)
    }
}
