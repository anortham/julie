//! Cross-project file watching for the daemon.
//!
//! Manages one `notify::RecommendedWatcher` per registered project so the daemon
//! detects file changes and triggers incremental re-indexing. Reuses filtering
//! and handler logic from `crate::watcher`.
//!
//! # Lifecycle
//!
//! - **Startup**: `start_watching` called for each `Ready` project.
//! - **Register**: New project registered via API starts a watcher.
//! - **Remove**: Project removed via API stops its watcher.
//! - **Shutdown**: `stop_all` drops every watcher and cancels background tasks.
//!
//! # Architecture
//!
//! Each project gets a `ProjectWatcher` containing:
//! - A `notify::RecommendedWatcher` for file system events
//! - A cancel flag to stop the background processing task
//! - Shared references to the project's database and search index
//!
//! File events are filtered (extension + ignore patterns), debounced (1s per-file),
//! and processed via the existing `watcher::handlers` static functions.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::SystemTime;

use notify::Watcher;
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tracing::{debug, error, info};

use crate::database::SymbolDatabase;
use crate::extractors::ExtractorManager;
use crate::search::SearchIndex;
use crate::watcher::filtering;
use crate::watcher::types::{FileChangeEvent, FileChangeType};

/// A file watcher for a single project, with its cancel flag and root path.
struct ProjectWatcher {
    /// The notify watcher handle — dropping this stops file system monitoring.
    _watcher: notify::RecommendedWatcher,
    /// Shared flag to cancel the background event-processing task.
    cancel_flag: Arc<AtomicBool>,
    /// Project root being watched (for logging).
    project_root: PathBuf,
}

/// Manages file watchers for all registered daemon projects.
///
/// Thread-safe: uses interior `tokio::sync::Mutex` for the watcher map so
/// callers don't need to hold the `DaemonState` write lock during potentially
/// slow watcher operations.
pub struct DaemonWatcherManager {
    /// Per-project watchers, keyed by workspace_id.
    watchers: TokioMutex<HashMap<String, ProjectWatcher>>,
    /// Shared extractor manager for re-indexing (stateless, cheap to clone).
    extractor_manager: Arc<ExtractorManager>,
    /// Pre-built supported extensions set (shared across all watchers).
    supported_extensions: HashSet<String>,
    /// Pre-built ignore patterns (shared across all watchers).
    ignore_patterns: Vec<glob::Pattern>,
}

impl DaemonWatcherManager {
    /// Create a new watcher manager.
    ///
    /// Builds the shared filtering state once, reused by all project watchers.
    pub fn new() -> anyhow::Result<Self> {
        let supported_extensions = filtering::build_supported_extensions();
        let ignore_patterns = filtering::build_ignore_patterns()?;

        Ok(Self {
            watchers: TokioMutex::new(HashMap::new()),
            extractor_manager: Arc::new(ExtractorManager::new()),
            supported_extensions,
            ignore_patterns,
        })
    }

    /// Start watching a project for file changes.
    ///
    /// Sets up a `notify::RecommendedWatcher` on the project root with recursive
    /// monitoring. File events are filtered, debounced, and fed to the existing
    /// `watcher::handlers` for incremental re-indexing.
    ///
    /// No-op if a watcher is already running for this workspace_id.
    ///
    /// # Arguments
    /// * `workspace_id` — unique project identifier
    /// * `project_root` — absolute path to the project directory
    /// * `db` — project's symbol database (from `LoadedWorkspace`)
    /// * `search_index` — project's Tantivy search index (from `LoadedWorkspace`)
    pub async fn start_watching(
        &self,
        workspace_id: String,
        project_root: PathBuf,
        db: Arc<StdMutex<SymbolDatabase>>,
        search_index: Option<Arc<StdMutex<SearchIndex>>>,
    ) {
        let mut watchers = self.watchers.lock().await;

        if watchers.contains_key(&workspace_id) {
            debug!(
                "Watcher already running for workspace {}, skipping",
                workspace_id
            );
            return;
        }

        info!(
            "Starting daemon file watcher for '{}' at {}",
            workspace_id,
            project_root.display()
        );

        match self.create_project_watcher(
            workspace_id.clone(),
            project_root.clone(),
            db,
            search_index,
        ) {
            Ok(pw) => {
                watchers.insert(workspace_id.clone(), pw);
                info!("Daemon file watcher started for '{}'", workspace_id);
            }
            Err(e) => {
                error!(
                    "Failed to start daemon file watcher for '{}': {}",
                    workspace_id, e
                );
            }
        }
    }

    /// Stop watching a specific project.
    ///
    /// Cancels the background task and drops the watcher handle.
    /// No-op if the project has no active watcher.
    pub async fn stop_watching(&self, workspace_id: &str) {
        let mut watchers = self.watchers.lock().await;

        if let Some(pw) = watchers.remove(workspace_id) {
            pw.cancel_flag.store(true, Ordering::Relaxed);
            info!(
                "Stopped daemon file watcher for '{}' ({})",
                workspace_id,
                pw.project_root.display()
            );
        } else {
            debug!(
                "No watcher running for workspace '{}', nothing to stop",
                workspace_id
            );
        }
    }

    /// Stop all project watchers. Called on daemon shutdown.
    pub async fn stop_all(&self) {
        let mut watchers = self.watchers.lock().await;
        let count = watchers.len();

        for (id, pw) in watchers.drain() {
            pw.cancel_flag.store(true, Ordering::Relaxed);
            debug!("Stopped daemon watcher for '{}'", id);
        }

        info!("Stopped {} daemon file watcher(s)", count);
    }

    /// Returns the list of workspace IDs with active watchers.
    pub async fn active_watchers(&self) -> Vec<String> {
        let watchers = self.watchers.lock().await;
        watchers.keys().cloned().collect()
    }

    /// Create a `ProjectWatcher` for a single project.
    ///
    /// Sets up the notify watcher, event channel, and background processing task.
    fn create_project_watcher(
        &self,
        workspace_id: String,
        project_root: PathBuf,
        db: Arc<StdMutex<SymbolDatabase>>,
        search_index: Option<Arc<StdMutex<SearchIndex>>>,
    ) -> anyhow::Result<ProjectWatcher> {
        let (tx, mut rx) = mpsc::unbounded_channel::<notify::Result<notify::Event>>();

        // Create the notify watcher
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Err(e) = tx.send(res) {
                error!("Failed to send daemon file event: {}", e);
            }
        })?;

        // Start recursive watching on the project root
        watcher
            .watch(&project_root, notify::RecursiveMode::Recursive)
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to watch {}: {}",
                    project_root.display(),
                    e
                )
            })?;

        let cancel_flag = Arc::new(AtomicBool::new(false));

        // Clone shared state for the event-processing task
        let supported_extensions = self.supported_extensions.clone();
        let ignore_patterns = self.ignore_patterns.clone();
        let extractor_manager = self.extractor_manager.clone();
        let cancel_flag_clone = cancel_flag.clone();
        let ws_id = workspace_id.clone();
        let root = project_root.clone();

        // Spawn the combined event-receiving + debounced-processing task
        let ctx = WatcherLoopContext {
            workspace_id: ws_id,
            workspace_root: root,
            cancel_flag: cancel_flag_clone,
            supported_extensions,
            ignore_patterns,
            db,
            search_index,
            extractor_manager,
        };
        tokio::spawn(async move {
            daemon_watcher_loop(ctx, &mut rx).await;
        });

        Ok(ProjectWatcher {
            _watcher: watcher,
            cancel_flag,
            project_root,
        })
    }
}

/// Bundled context for the daemon watcher loop to avoid excessive parameter counts.
struct WatcherLoopContext {
    workspace_id: String,
    workspace_root: PathBuf,
    cancel_flag: Arc<AtomicBool>,
    supported_extensions: HashSet<String>,
    ignore_patterns: Vec<glob::Pattern>,
    db: Arc<StdMutex<SymbolDatabase>>,
    search_index: Option<Arc<StdMutex<SearchIndex>>>,
    extractor_manager: Arc<ExtractorManager>,
}

/// Main event loop for a single project's daemon watcher.
///
/// Receives file system events, filters and queues them, then processes the
/// queue every second with per-file deduplication.
async fn daemon_watcher_loop(
    ctx: WatcherLoopContext,
    rx: &mut mpsc::UnboundedReceiver<notify::Result<notify::Event>>,
) {
    use tokio::time::{Duration, interval};

    let index_queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));
    let last_processed: Arc<TokioMutex<HashMap<PathBuf, SystemTime>>> =
        Arc::new(TokioMutex::new(HashMap::new()));

    let mut tick = interval(Duration::from_secs(1));

    debug!("Daemon watcher loop started for '{}'", ctx.workspace_id);

    loop {
        tokio::select! {
            // Branch 1: Receive file system events (non-blocking)
            event_result = rx.recv() => {
                if ctx.cancel_flag.load(Ordering::Relaxed) {
                    debug!("Daemon watcher for '{}' cancelled (event branch)", ctx.workspace_id);
                    break;
                }
                match event_result {
                    Some(Ok(event)) => {
                        if let Err(e) = crate::watcher::events::process_file_system_event(
                            &ctx.supported_extensions,
                            &ctx.ignore_patterns,
                            index_queue.clone(),
                            event,
                        ).await {
                            error!("[{}] Error processing file event: {}", ctx.workspace_id, e);
                        }
                    }
                    Some(Err(e)) => {
                        error!("[{}] File watcher error: {}", ctx.workspace_id, e);
                    }
                    None => {
                        // Channel closed — watcher was dropped
                        debug!("Daemon watcher channel closed for '{}'", ctx.workspace_id);
                        break;
                    }
                }
            }
            // Branch 2: Process queued events every tick (debounce)
            _ = tick.tick() => {
                if ctx.cancel_flag.load(Ordering::Relaxed) {
                    debug!("Daemon watcher for '{}' cancelled (tick branch)", ctx.workspace_id);
                    break;
                }

                process_queued_events(
                    &ctx.workspace_id,
                    &ctx.workspace_root,
                    &index_queue,
                    &last_processed,
                    &ctx.db,
                    &ctx.extractor_manager,
                    ctx.search_index.as_ref(),
                ).await;
            }
        }
    }

    debug!("Daemon watcher loop exited for '{}'", ctx.workspace_id);
}

/// Process all queued file change events with per-file deduplication.
///
/// Mirrors the processing logic from `IncrementalIndexer::start_watching` but
/// without embedding support (daemon re-indexing is for search only).
async fn process_queued_events(
    workspace_id: &str,
    workspace_root: &Path,
    index_queue: &Arc<TokioMutex<VecDeque<FileChangeEvent>>>,
    last_processed: &Arc<TokioMutex<HashMap<PathBuf, SystemTime>>>,
    db: &Arc<StdMutex<SymbolDatabase>>,
    extractor_manager: &Arc<ExtractorManager>,
    search_index: Option<&Arc<StdMutex<SearchIndex>>>,
) {
    use std::time::Duration;
    use crate::watcher::handlers;

    while let Some(event) = {
        let mut queue = index_queue.lock().await;
        queue.pop_front()
    } {
        // Deduplication: skip if processed within the last second
        let should_skip = {
            let mut last_proc = last_processed.lock().await;
            let now = SystemTime::now();

            if let Some(last_time) = last_proc.get(&event.path) {
                if let Ok(elapsed) = now.duration_since(*last_time) {
                    if elapsed < Duration::from_secs(1) {
                        debug!(
                            "[{}] Skipping duplicate event for {:?} ({}ms ago)",
                            workspace_id,
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

        debug!("[{}] Processing file change: {:?}", workspace_id, event.path);

        match event.change_type {
            FileChangeType::Created | FileChangeType::Modified => {
                if let Err(e) = handlers::handle_file_created_or_modified_static(
                    event.path,
                    db,
                    extractor_manager,
                    workspace_root,
                    search_index,
                )
                .await
                {
                    error!(
                        "[{}] Failed to handle file change: {}",
                        workspace_id, e
                    );
                }
            }
            FileChangeType::Deleted => {
                // Guard: atomic save pattern — file still exists after DELETE event
                if event.path.exists() {
                    debug!(
                        "[{}] Skipping DELETE for {} (file still exists, likely atomic save)",
                        workspace_id,
                        event.path.display()
                    );
                    // Clear dedup so follow-up Create/Modify isn't skipped
                    last_processed.lock().await.remove(&event.path);
                } else if let Err(e) = handlers::handle_file_deleted_static(
                    event.path,
                    db,
                    workspace_root,
                )
                .await
                {
                    error!(
                        "[{}] Failed to handle file deletion: {}",
                        workspace_id, e
                    );
                }
            }
            FileChangeType::Renamed { from, to } => {
                if let Err(e) = handlers::handle_file_renamed_static(
                    from,
                    to,
                    db,
                    extractor_manager,
                    workspace_root,
                    search_index,
                )
                .await
                {
                    error!(
                        "[{}] Failed to handle file rename: {}",
                        workspace_id, e
                    );
                }
            }
        }
    }
}
