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
async fn dispatch_file_event(
    event: FileChangeEvent,
    db: &Arc<StdMutex<SymbolDatabase>>,
    extractor_manager: &Arc<ExtractorManager>,
    search_index: &Option<Arc<StdMutex<crate::search::SearchIndex>>>,
    embedding_provider: &Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
    workspace_root: &std::path::Path,
    lang_configs: &Arc<crate::search::language_config::LanguageConfigs>,
    tantivy_dirty: &Arc<StdMutex<std::collections::HashSet<String>>>,
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
                Ok(tantivy_ok) => {
                    // Fix B-b: track Tantivy failures for retry on next tick
                    if !tantivy_ok {
                        if let Some(ref rel) = rel_path {
                            let mut dirty = tantivy_dirty.lock().unwrap_or_else(|p| p.into_inner());
                            dirty.insert(rel.clone());
                            warn!("Tantivy update failed for {}; queued for retry", rel);
                        }
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
            if let Ok(ref rel_from) =
                crate::utils::paths::to_relative_unix_style(&from, workspace_root)
            {
                if let Ok(mut db_guard) = db.lock() {
                    let _ = db_guard.delete_embeddings_for_file(rel_from);
                }
                // Clear old path from dirty-retry set (file no longer exists at old path).
                tantivy_dirty
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(rel_from);
            }
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
                    warn!("Failed to handle file rename: {}", e);
                }
                Ok(tantivy_ok) => {
                    // Track Tantivy failure on rename's create side for dirty-retry.
                    if !tantivy_ok {
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
    pub fn new(
        workspace_root: PathBuf,
        db: Arc<StdMutex<SymbolDatabase>>,
        extractor_manager: Arc<ExtractorManager>,
        search_index: Option<Arc<StdMutex<crate::search::SearchIndex>>>,
        embedding_provider: SharedEmbeddingProvider,
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
        let cancel_flag_events = self.cancel_flag.clone();
        let needs_rescan_for_events = self.needs_rescan.clone();

        let event_handle = tokio::spawn(async move {
            info!("File system event detector started");
            while let Some(event_result) = rx.recv().await {
                if cancel_flag_events.load(Ordering::Acquire) {
                    info!("Event detector cancelled, exiting");
                    break;
                }
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
        let db = self.db.clone();
        let extractor_manager = self.extractor_manager.clone();
        let search_index = self.search_index.clone();
        let embedding_provider = self.embedding_provider.clone();
        let lang_configs = self.lang_configs.clone();
        let queue_for_processing = self.index_queue.clone();
        let last_processed = self.last_processed.clone();
        let workspace_root = self.workspace_root.clone();
        let cancel_flag_queue = self.cancel_flag.clone();
        let pause_flag_queue = self.pause_flag.clone();
        let needs_rescan = self.needs_rescan.clone();
        let tantivy_dirty = self.tantivy_dirty.clone();
        let supported_extensions_queue = self.supported_extensions.clone();

        let queue_handle = tokio::spawn(async move {
            use tokio::time::{Duration, interval};
            let mut tick = interval(Duration::from_secs(1)); // Process queue every second

            info!("Background queue processor started");
            loop {
                tick.tick().await;

                if cancel_flag_queue.load(Ordering::Acquire) {
                    // Drain remaining queued events before exiting so we don't
                    // lose in-flight work (e.g., edits queued just before shutdown).
                    let remaining = queue_for_processing.lock().await.len();
                    if remaining > 0 {
                        info!(
                            "Queue processor shutting down, draining {} remaining events",
                            remaining
                        );
                        while let Some(event) = queue_for_processing.lock().await.pop_front() {
                            let provider_snap = embedding_provider
                                .read()
                                .unwrap_or_else(|p| p.into_inner())
                                .clone();
                            dispatch_file_event(
                                event,
                                &db,
                                &extractor_manager,
                                &search_index,
                                &provider_snap,
                                &workspace_root,
                                &lang_configs,
                                &tantivy_dirty,
                            )
                            .await;
                        }
                        // Final Tantivy commit for drained events
                        if let Some(ref si) = search_index {
                            let si_arc = Arc::clone(si);
                            let _ = tokio::task::spawn_blocking(move || {
                                let idx = si_arc.lock().unwrap_or_else(|p| p.into_inner());
                                if let Err(e) = idx.commit() {
                                    warn!("Failed to commit Tantivy during shutdown drain: {}", e);
                                }
                            })
                            .await;
                        }
                    }
                    info!("Queue processor cancelled, exiting");
                    break;
                }

                // Fix C part a: skip dispatch while paused (catch-up indexing in progress).
                // Events accumulate in the queue and are processed after resume().
                if pause_flag_queue.load(Ordering::Acquire) {
                    continue;
                }

                // Fix B-b: Retry Tantivy for any files that failed in previous ticks.
                // SQLite already has correct data; this just syncs Tantivy to match it.
                {
                    let dirty_paths: Vec<String> = {
                        let d = tantivy_dirty.lock().unwrap_or_else(|p| p.into_inner());
                        d.iter().cloned().collect()
                    };
                    if !dirty_paths.is_empty() {
                        if let Some(ref si) = search_index {
                            for rel_path in dirty_paths {
                                let (symbol_docs, file_doc) = {
                                    let db_guard = db.lock().unwrap_or_else(|p| p.into_inner());
                                    let symbols = db_guard
                                        .get_symbols_for_file(&rel_path)
                                        .unwrap_or_default();
                                    let content = db_guard
                                        .get_file_content(&rel_path)
                                        .unwrap_or(None)
                                        .unwrap_or_default();
                                    let language = symbols
                                        .first()
                                        .map(|s| s.language.clone())
                                        .unwrap_or_else(|| "unknown".to_string());
                                    let docs: Vec<_> = symbols
                                        .iter()
                                        .map(crate::search::SymbolDocument::from_symbol)
                                        .collect();
                                    let fd = crate::search::FileDocument {
                                        file_path: rel_path.clone(),
                                        content,
                                        language,
                                    };
                                    (docs, fd)
                                };
                                let si_arc = Arc::clone(si);
                                let rel_clone = rel_path.clone();
                                let retry_result = tokio::task::spawn_blocking(move || {
                                    let idx = si_arc.lock().unwrap_or_else(|p| p.into_inner());
                                    idx.remove_by_file_path(&rel_clone)?;
                                    for doc in &symbol_docs {
                                        idx.add_symbol(doc)?;
                                    }
                                    idx.add_file_content(&file_doc)?;
                                    Ok::<(), anyhow::Error>(())
                                })
                                .await;
                                match retry_result {
                                    Ok(Ok(())) => {
                                        tantivy_dirty
                                            .lock()
                                            .unwrap_or_else(|p| p.into_inner())
                                            .remove(&rel_path);
                                        info!("Tantivy retry succeeded for {}", rel_path);
                                    }
                                    Ok(Err(e)) => {
                                        warn!("Tantivy retry failed for {}: {}", rel_path, e)
                                    }
                                    Err(e) => {
                                        warn!("Tantivy retry task panicked for {}: {}", rel_path, e)
                                    }
                                }
                            }
                        }
                    }
                }

                // Process all items currently in the queue
                let queue_size = {
                    let queue = queue_for_processing.lock().await;
                    queue.len()
                };

                if queue_size > 0 {
                    debug!("Processing {} queued file events", queue_size);
                }

                let mut processed_count = 0usize;
                // Cap iterations at queue_size to prevent hot-spinning.
                // When all events are within the dedup window, they're pushed back
                // and we exit after one pass. Without this cap, a single deduped
                // event would pop/push/continue in an infinite loop at CPU speed.
                let max_this_tick = queue_size;
                let mut iterations = 0;
                while iterations < max_this_tick {
                    let event = match {
                        let mut queue = queue_for_processing.lock().await;
                        queue.pop_front()
                    } {
                        Some(e) => e,
                        None => break,
                    };
                    iterations += 1;
                    // Deduplication: Skip if we processed this file very recently (within 1 second)
                    // This prevents duplicate processing when notify fires multiple events (Create + Modify)
                    let should_skip = {
                        let mut last_proc = last_processed.lock().await;
                        let now = SystemTime::now();

                        if let Some(last_time) = last_proc.get(&event.path) {
                            if let Ok(elapsed) = now.duration_since(*last_time) {
                                if elapsed < Duration::from_secs(1) {
                                    debug!(
                                        "Skipping duplicate event for {:?} (processed {}ms ago)",
                                        event.path,
                                        elapsed.as_millis()
                                    );
                                    true
                                } else {
                                    last_proc.insert(event.path.clone(), now);
                                    false
                                }
                            } else {
                                last_proc.insert(event.path.clone(), now);
                                false
                            }
                        } else {
                            last_proc.insert(event.path.clone(), now);
                            false
                        }
                    };

                    if should_skip {
                        // Fix C (HOL blocking): re-queue and CONTINUE (not break) so
                        // subsequent events for other files are not blocked by a single
                        // deduped event. Only the re-queued file waits for the next tick;
                        // everything behind it can still be processed now.
                        let mut queue = queue_for_processing.lock().await;
                        queue.push_back(event);
                        continue; // Fix C: was 'break', which stalled the whole queue
                    }

                    info!("Background task processing: {:?}", event.path);

                    let provider_snapshot = embedding_provider
                        .read()
                        .unwrap_or_else(|p| p.into_inner())
                        .clone();

                    // Fix F: dispatch_file_event returns Some(path) when an atomic-delete
                    // is skipped. Inline the dedup-map removal here instead of spawning
                    // a detached task (eliminates the detached tokio::spawn).
                    let atomic_delete_path = dispatch_file_event(
                        event,
                        &db,
                        &extractor_manager,
                        &search_index,
                        &provider_snapshot,
                        &workspace_root,
                        &lang_configs,
                        &tantivy_dirty,
                    )
                    .await;

                    if let Some(path) = atomic_delete_path {
                        last_processed.lock().await.remove(&path);
                    }

                    processed_count += 1;
                }

                // Evict stale dedup entries older than 2 seconds to prevent unbounded growth.
                // Entries from the last 1 second are still needed for dedup; 2s gives a safety margin.
                {
                    let mut last_proc = last_processed.lock().await;
                    last_proc.retain(|_, t| {
                        t.elapsed()
                            .map(|e| e < Duration::from_secs(2))
                            .unwrap_or(false)
                    });
                }

                // Batch-commit Tantivy only if we actually processed events
                // (not just queued or skipped via dedup).
                if processed_count > 0 {
                    if let Some(ref search_index) = search_index {
                        let si = Arc::clone(search_index);
                        let _ = tokio::task::spawn_blocking(move || {
                            let idx = match si.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            if let Err(e) = idx.commit() {
                                warn!("Failed to commit Tantivy batch: {}", e);
                            }
                        })
                        .await;
                    }
                }

                // Fix C (overflow recovery): after the queue drains, trigger a
                // workspace-wide staleness scan to recover any events that were dropped
                // when the queue exceeded 1000 items (e.g., large git checkout).
                let queue_now_empty = queue_for_processing.lock().await.is_empty();
                if queue_now_empty && needs_rescan.load(Ordering::Acquire) {
                    needs_rescan.store(false, Ordering::Release);
                    info!(
                        "Queue overflow detected: full workspace rescan for staleness + new files"
                    );

                    // 1. Check all indexed files for modifications or deletions.
                    let indexed_files = {
                        let db_guard = db.lock().unwrap_or_else(|p| p.into_inner());
                        db_guard.get_all_indexed_files().unwrap_or_default()
                    };
                    let indexed_set: std::collections::HashSet<String> =
                        indexed_files.iter().cloned().collect();

                    let provider_snap = embedding_provider
                        .read()
                        .unwrap_or_else(|p| p.into_inner())
                        .clone();
                    for rel_path in &indexed_files {
                        let abs_path = workspace_root.join(std::path::Path::new(rel_path));
                        let change_type = if abs_path.is_file() {
                            FileChangeType::Modified
                        } else {
                            FileChangeType::Deleted
                        };
                        let rescan_event = FileChangeEvent {
                            path: abs_path,
                            change_type,
                            timestamp: SystemTime::now(),
                        };
                        dispatch_file_event(
                            rescan_event,
                            &db,
                            &extractor_manager,
                            &search_index,
                            &provider_snap,
                            &workspace_root,
                            &lang_configs,
                            &tantivy_dirty,
                        )
                        .await;
                    }

                    // 2. Walk filesystem to discover NEW files not in DB.
                    // Overflow may have dropped Create events for brand-new files.
                    let mut new_file_count = 0usize;
                    let walker = ignore::WalkBuilder::new(&workspace_root)
                        .hidden(true)
                        .git_ignore(true)
                        .build();
                    for entry in walker
                        .filter_map(|e| e.ok())
                        .filter(|e| e.file_type().map_or(false, |ft| ft.is_file()))
                    {
                        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                            if supported_extensions_queue.contains(ext) {
                                if let Ok(rel) = crate::utils::paths::to_relative_unix_style(
                                    entry.path(),
                                    &workspace_root,
                                ) {
                                    if !indexed_set.contains(&rel) {
                                        let event = FileChangeEvent {
                                            path: entry.into_path(),
                                            change_type: FileChangeType::Created,
                                            timestamp: SystemTime::now(),
                                        };
                                        dispatch_file_event(
                                            event,
                                            &db,
                                            &extractor_manager,
                                            &search_index,
                                            &provider_snap,
                                            &workspace_root,
                                            &lang_configs,
                                            &tantivy_dirty,
                                        )
                                        .await;
                                        new_file_count += 1;
                                    }
                                }
                            }
                        }
                    }
                    info!(
                        "Post-overflow rescan: checked {} indexed files, discovered {} new files",
                        indexed_files.len(),
                        new_file_count
                    );
                    // Commit Tantivy after rescan batch
                    if let Some(ref si) = search_index {
                        let si_arc = Arc::clone(si);
                        let _ = tokio::task::spawn_blocking(move || {
                            let idx = si_arc.lock().unwrap_or_else(|p| p.into_inner());
                            if let Err(e) = idx.commit() {
                                warn!("Failed to commit Tantivy after rescan: {}", e);
                            }
                        })
                        .await;
                    }
                }
            }
        });
        self.queue_task = Some(queue_handle);

        info!("File watcher started successfully with background queue processing");
        Ok(())
    }

    /// Process any pending file changes from the queue
    pub async fn process_pending_changes(&self) -> Result<()> {
        let mut processed_count = 0usize;
        while let Some(event) = {
            let mut queue = self.index_queue.lock().await;
            queue.pop_front()
        } {
            let provider_snapshot = self
                .embedding_provider
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            // dispatch returns Some(path) for atomic-save skips; no dedup tracking here.
            dispatch_file_event(
                event,
                &self.db,
                &self.extractor_manager,
                &self.search_index,
                &provider_snapshot,
                &self.workspace_root,
                &self.lang_configs,
                &self.tantivy_dirty,
            )
            .await;
            processed_count += 1;
        }

        // Batch-commit Tantivy only if we actually dispatched events.
        if processed_count > 0 {
            if let Some(ref search_index) = self.search_index {
                let si = Arc::clone(search_index);
                let _ = tokio::task::spawn_blocking(move || {
                    let idx = match si.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if let Err(e) = idx.commit() {
                        warn!("Failed to commit Tantivy batch: {}", e);
                    }
                })
                .await;
            }
        }

        Ok(())
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
        debug!("File watcher paused");
    }

    /// Resume event dispatch after a `pause()`.
    pub fn resume(&self) {
        self.pause_flag.store(false, Ordering::Release);
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

// Test integration with new module structure
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supported_extensions() {
        let extensions = filtering::build_supported_extensions();
        assert!(extensions.contains("rs"));
        assert!(extensions.contains("ts"));
        assert!(extensions.contains("py"));
        assert!(!extensions.contains("txt"));
    }

    #[test]
    fn test_gitignore_matcher() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "*.log\nvendor/\n").unwrap();

        let gitignore = filtering::build_gitignore_matcher(dir.path()).unwrap();
        assert!(
            gitignore
                .matched_path_or_any_parents("debug.log", false)
                .is_ignore()
        );
    }
}
