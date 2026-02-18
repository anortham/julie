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

mod events;
pub mod filtering; // Public for tests
pub mod handlers; // Public for tests
pub mod types;

use anyhow::{Context, Result};
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

/// Manages incremental indexing with real-time file watching
pub struct IncrementalIndexer {
    watcher: Option<notify::RecommendedWatcher>,
    db: Arc<StdMutex<SymbolDatabase>>,
    extractor_manager: Arc<ExtractorManager>,
    search_index: Option<Arc<StdMutex<crate::search::SearchIndex>>>,

    // Processing queues
    pub(crate) index_queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>>,

    // Event deduplication: Track recently processed files to avoid duplicate processing
    // Key: file path, Value: last processed timestamp
    last_processed: Arc<TokioMutex<HashMap<PathBuf, SystemTime>>>,

    // File filters
    supported_extensions: HashSet<String>,
    ignore_patterns: Vec<glob::Pattern>,

    // Configuration
    workspace_root: PathBuf,

    /// Shared flag checked by spawned tasks — when set to true, tasks exit their loops.
    cancel_flag: Arc<AtomicBool>,
}

impl IncrementalIndexer {
    /// Create a new incremental indexer for the given workspace
    pub fn new(
        workspace_root: PathBuf,
        db: Arc<StdMutex<SymbolDatabase>>,
        extractor_manager: Arc<ExtractorManager>,
        search_index: Option<Arc<StdMutex<crate::search::SearchIndex>>>,
    ) -> Result<Self> {
        let supported_extensions = filtering::build_supported_extensions();
        let ignore_patterns = filtering::build_ignore_patterns()?;

        Ok(Self {
            watcher: None,
            db,
            extractor_manager,
            search_index,
            index_queue: Arc::new(TokioMutex::new(VecDeque::new())),
            last_processed: Arc::new(TokioMutex::new(HashMap::new())),
            supported_extensions,
            ignore_patterns,
            workspace_root,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        })
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
        let ignore_patterns = self.ignore_patterns.clone();
        let index_queue = self.index_queue.clone();
        let cancel_flag_events = self.cancel_flag.clone();

        tokio::spawn(async move {
            info!("File system event detector started");
            while let Some(event_result) = rx.recv().await {
                if cancel_flag_events.load(Ordering::Relaxed) {
                    info!("Event detector cancelled, exiting");
                    break;
                }
                match event_result {
                    Ok(event) => {
                        debug!("File system event detected: {:?}", event);
                        if let Err(e) = events::process_file_system_event(
                            &supported_extensions,
                            &ignore_patterns,
                            index_queue.clone(),
                            event,
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

        // Spawn background task to process queued events
        // Clone all the components needed for processing
        let db = self.db.clone();
        let extractor_manager = self.extractor_manager.clone();
        let search_index = self.search_index.clone();
        let queue_for_processing = self.index_queue.clone();
        let last_processed = self.last_processed.clone();
        let workspace_root = self.workspace_root.clone();
        let cancel_flag_queue = self.cancel_flag.clone();

        tokio::spawn(async move {
            use tokio::time::{Duration, interval};
            let mut tick = interval(Duration::from_secs(1)); // Process queue every second

            info!("Background queue processor started");
            loop {
                tick.tick().await;

                if cancel_flag_queue.load(Ordering::Relaxed) {
                    info!("Queue processor cancelled, exiting");
                    break;
                }

                // Process all items currently in the queue
                let queue_size = {
                    let queue = queue_for_processing.lock().await;
                    queue.len()
                };

                if queue_size > 0 {
                    debug!("Processing {} queued file events", queue_size);
                }

                while let Some(event) = {
                    let mut queue = queue_for_processing.lock().await;
                    queue.pop_front()
                } {
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
                        continue;
                    }

                    info!("Background task processing: {:?}", event.path);
                    if let Err(e) = match event.change_type {
                        FileChangeType::Created | FileChangeType::Modified => {
                            handlers::handle_file_created_or_modified_static(
                                event.path,
                                &db,
                                &extractor_manager,
                                &workspace_root,
                                search_index.as_ref(),
                            )
                            .await
                        }
                        FileChangeType::Deleted => {
                            handlers::handle_file_deleted_static(
                                event.path,
                                &db,
                                &workspace_root,
                            )
                            .await
                        }
                        FileChangeType::Renamed { from, to } => {
                            handlers::handle_file_renamed_static(
                                from,
                                to,
                                &db,
                                &extractor_manager,
                                &workspace_root,
                                search_index.as_ref(),
                            )
                            .await
                        }
                    } {
                        error!("Failed to handle file change: {}", e);
                    }
                }
            }
        });

        info!("File watcher started successfully with background queue processing");
        Ok(())
    }

    /// Process any pending file changes from the queue
    pub async fn process_pending_changes(&self) -> Result<()> {
        // Process all items currently in the queue
        while let Some(event) = {
            let mut queue = self.index_queue.lock().await;
            queue.pop_front()
        } {
            if let Err(e) = match event.change_type {
                FileChangeType::Created | FileChangeType::Modified => {
                    handlers::handle_file_created_or_modified_static(
                        event.path,
                        &self.db,
                        &self.extractor_manager,
                        &self.workspace_root,
                        self.search_index.as_ref(),
                    )
                    .await
                }
                FileChangeType::Deleted => {
                    handlers::handle_file_deleted_static(
                        event.path,
                        &self.db,
                        &self.workspace_root,
                    )
                    .await
                }
                FileChangeType::Renamed { from, to } => {
                    handlers::handle_file_renamed_static(
                        from,
                        to,
                        &self.db,
                        &self.extractor_manager,
                        &self.workspace_root,
                        self.search_index.as_ref(),
                    )
                    .await
                }
            } {
                error!("Failed to handle file change: {}", e);
            }
        }
        Ok(())
    }

    /// Stop the file watcher and signal spawned tasks to exit.
    pub async fn stop(&mut self) -> Result<()> {
        // Signal spawned tasks to exit their loops
        self.cancel_flag.store(true, Ordering::Relaxed);

        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
            info!("File watcher stopped");
        }
        Ok(())
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
    fn test_ignore_patterns() {
        let patterns = filtering::build_ignore_patterns().unwrap();
        assert!(!patterns.is_empty());
    }
}
