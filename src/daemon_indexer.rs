//! Background indexing pipeline for the daemon.
//!
//! Provides a sequential indexing queue: API endpoints, file watchers, and startup
//! logic send `IndexRequest` messages through an `mpsc` channel, and a single
//! background tokio task processes them one at a time to avoid resource contention.
//!
//! # Status flow
//!
//! ```text
//! Registered ──► Indexing ──► Ready
//!                    │
//!                    └──► Error(message)
//! ```
//!
//! # Architecture
//!
//! - `IndexingSender` — cloneable handle for submitting requests (stored in `AppState`).
//! - `spawn_indexing_worker` — spawns the background consumer task.
//! - The worker creates a temporary `JulieServerHandler` per job, runs the full
//!   indexing pipeline, then updates both `DaemonState` (loaded workspace) and
//!   `GlobalRegistry` (persisted status).

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};
use tracing::{error, info, warn};

use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};
use crate::handler::JulieServerHandler;
use crate::registry::GlobalRegistry;
use crate::tools::ManageWorkspaceTool;
use crate::workspace::JulieWorkspace;

/// A request to index (or re-index) a project.
#[derive(Debug, Clone)]
pub struct IndexRequest {
    /// The workspace ID in the global registry.
    pub workspace_id: String,
    /// Absolute path to the project directory.
    pub project_path: PathBuf,
    /// If true, force a full re-index even if indexes already exist.
    pub force: bool,
}

/// Cloneable sender handle for submitting indexing requests.
///
/// Stored in `AppState` so API handlers, file watchers, and startup code can
/// all submit requests without holding any locks.
pub type IndexingSender = mpsc::Sender<IndexRequest>;

/// Channel capacity — how many requests can be buffered before back-pressure.
/// 64 is generous; in practice the daemon rarely has more than a handful of
/// projects queued simultaneously.
const CHANNEL_CAPACITY: usize = 64;

/// Create the indexing channel and spawn the background worker.
///
/// Returns the sender half for callers to submit requests. The receiver is
/// moved into the spawned task.
pub fn spawn_indexing_worker(
    registry: Arc<RwLock<GlobalRegistry>>,
    daemon_state: Arc<RwLock<DaemonState>>,
    julie_home: PathBuf,
    cancellation_token: tokio_util::sync::CancellationToken,
) -> IndexingSender {
    let (tx, rx) = mpsc::channel::<IndexRequest>(CHANNEL_CAPACITY);

    tokio::spawn(indexing_worker_loop(
        rx,
        registry,
        daemon_state,
        julie_home,
        cancellation_token,
    ));

    tx
}

/// The background worker loop — processes indexing requests sequentially.
async fn indexing_worker_loop(
    mut rx: mpsc::Receiver<IndexRequest>,
    registry: Arc<RwLock<GlobalRegistry>>,
    daemon_state: Arc<RwLock<DaemonState>>,
    julie_home: PathBuf,
    cancellation_token: tokio_util::sync::CancellationToken,
) {
    info!("Background indexing worker started");

    loop {
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                info!("Background indexing worker cancelled by shutdown signal");
                break;
            }
            request = rx.recv() => {
                match request {
                    Some(request) => {
                        info!(
                            "Processing index request: workspace_id={}, path={}, force={}",
                            request.workspace_id,
                            request.project_path.display(),
                            request.force,
                        );

                        process_index_request(
                            &request,
                            &registry,
                            &daemon_state,
                            &julie_home,
                        )
                        .await;
                    }
                    None => {
                        info!("Background indexing worker stopped (channel closed)");
                        break;
                    }
                }
            }
        }
    }
}

/// Process a single indexing request.
///
/// 1. Mark status as `Indexing` in the registry.
/// 2. Create a `JulieServerHandler` for this project.
/// 3. Initialize workspace + run indexing.
/// 4. On success: update registry to `Ready`, update `DaemonState` with loaded workspace.
/// 5. On failure: update registry to `Error`.
async fn process_index_request(
    request: &IndexRequest,
    registry: &Arc<RwLock<GlobalRegistry>>,
    daemon_state: &Arc<RwLock<DaemonState>>,
    julie_home: &PathBuf,
) {
    // Step 1: Mark as Indexing
    {
        let mut reg = registry.write().await;
        reg.mark_indexing(&request.workspace_id);
        if let Err(e) = reg.save(julie_home) {
            warn!("Failed to persist registry after marking Indexing: {}", e);
        }
    }

    // Update DaemonState status to Indexing too
    {
        let mut ds = daemon_state.write().await;
        if let Some(loaded) = ds.workspaces.get_mut(&request.workspace_id) {
            loaded.status = WorkspaceLoadStatus::Indexing;
        }
    }

    // Step 1.5: Release existing Tantivy file lock before indexing.
    //
    // run_indexing_pipeline creates a temporary JulieServerHandler with its own
    // SearchIndex on the same Tantivy directory. If the existing workspace's
    // SearchIndex still holds a writer (created by the watcher), the new instance
    // will get LockBusy errors. Fix: stop the watcher, then shut down the writer.
    {
        let ds = daemon_state.read().await;

        // Stop the file watcher (it holds an Arc to the existing SearchIndex)
        ds.watcher_manager
            .stop_watching(&request.workspace_id)
            .await;

        // Shut down the existing SearchIndex to release the Tantivy file lock.
        // Reads (searches) continue to work; only writes are blocked.
        if let Some(loaded) = ds.workspaces.get(&request.workspace_id) {
            if let Some(ref search_index) = loaded.workspace.search_index {
                let idx = search_index.lock().unwrap_or_else(|p| p.into_inner());
                if let Err(e) = idx.shutdown() {
                    warn!(
                        "Failed to shut down search index before re-indexing '{}': {}",
                        request.workspace_id, e
                    );
                }
            }
        }
    }

    // Step 2: Create a handler and run indexing
    let result = run_indexing_pipeline(&request.project_path, request.force).await;

    match result {
        Ok(indexing_outcome) => {
            info!(
                "Indexing succeeded for '{}': {} files, {} symbols",
                request.workspace_id,
                indexing_outcome.file_count,
                indexing_outcome.symbol_count,
            );

            // Step 3a: Mark Ready in registry
            {
                let mut reg = registry.write().await;
                reg.mark_ready(
                    &request.workspace_id,
                    indexing_outcome.symbol_count,
                    indexing_outcome.file_count,
                );
                if let Err(e) = reg.save(julie_home) {
                    warn!("Failed to persist registry after marking Ready: {}", e);
                }
            }

            // Step 3b: Update DaemonState with loaded workspace + create MCP service
            {
                let mut ds = daemon_state.write().await;
                ds.workspaces.insert(
                    request.workspace_id.clone(),
                    LoadedWorkspace {
                        workspace: indexing_outcome.workspace,
                        status: WorkspaceLoadStatus::Ready,
                        path: request.project_path.clone(),
                    },
                );

                // Create/replace MCP service for this workspace
                let mcp_service = DaemonState::create_workspace_mcp_service(
                    request.project_path.clone(),
                    &ds.cancellation_token,
                    daemon_state.clone(),
                );
                ds.mcp_services
                    .insert(request.workspace_id.clone(), mcp_service);
            }

            // Step 3c: Start file watcher for this workspace
            {
                let ds = daemon_state.read().await;
                ds.start_watcher_if_ready(&request.workspace_id).await;
            }
        }
        Err(e) => {
            let error_msg = format!("{:#}", e);
            error!(
                "Indexing failed for '{}': {}",
                request.workspace_id, error_msg
            );

            // Mark Error in registry
            {
                let mut reg = registry.write().await;
                reg.mark_error(&request.workspace_id, error_msg.clone());
                if let Err(e) = reg.save(julie_home) {
                    warn!("Failed to persist registry after marking Error: {}", e);
                }
            }

            // Update DaemonState status
            {
                let mut ds = daemon_state.write().await;
                if let Some(loaded) = ds.workspaces.get_mut(&request.workspace_id) {
                    loaded.status = WorkspaceLoadStatus::Error(error_msg);
                }
            }
        }
    }
}

/// Outcome of a successful indexing run.
struct IndexingOutcome {
    /// The fully-loaded workspace (with db + search index).
    workspace: JulieWorkspace,
    /// Total symbols indexed.
    symbol_count: u64,
    /// Total files indexed.
    file_count: u64,
}

/// Run the full indexing pipeline for a project path.
///
/// Creates a temporary `JulieServerHandler`, initializes the workspace, runs
/// indexing, and returns the loaded workspace + stats.
async fn run_indexing_pipeline(
    project_path: &PathBuf,
    force: bool,
) -> anyhow::Result<IndexingOutcome> {
    // Create a handler pointed at this project
    let handler = JulieServerHandler::new(project_path.clone()).await?;

    // Initialize workspace (creates .julie/ if needed, or loads existing)
    handler
        .initialize_workspace_with_force(
            Some(project_path.to_string_lossy().to_string()),
            force,
        )
        .await?;

    // Run indexing via ManageWorkspaceTool
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(project_path.to_string_lossy().to_string()),
        name: None,
        workspace_id: None,
        force: Some(force),
        detailed: None,
    };

    index_tool.call_tool_with_options(&handler, false).await?;

    // Extract the workspace from the handler
    let workspace = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("Workspace not available after indexing"))?;

    // Get symbol and file counts from the workspace database
    let (symbol_count, file_count) = if let Some(db_arc) = &workspace.db {
        let db = match db_arc.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!("Database mutex poisoned after indexing, recovering");
                poisoned.into_inner()
            }
        };
        let symbols = db.count_symbols_for_workspace().unwrap_or(0) as u64;
        let files = db.get_all_indexed_files().map(|f| f.len()).unwrap_or(0) as u64;
        (symbols, files)
    } else {
        (0, 0)
    };

    Ok(IndexingOutcome {
        workspace,
        symbol_count,
        file_count,
    })
}
