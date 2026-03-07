//! Daemon-wide shared state for multi-project workspace management.
//!
//! `DaemonState` holds the loaded workspaces for all registered projects,
//! along with per-workspace MCP services. It is created on daemon startup
//! and shared (via `Arc`) across all axum handlers and MCP sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use rmcp::transport::streamable_http_server::{
    StreamableHttpService,
    StreamableHttpServerConfig,
    session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn, debug};

use crate::daemon_watcher::DaemonWatcherManager;
use crate::handler::JulieServerHandler;
use crate::registry::{GlobalRegistry, ProjectStatus};
use crate::workspace::JulieWorkspace;

/// Status of a workspace in the daemon's loaded workspace pool.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceLoadStatus {
    /// Workspace loaded successfully with database and search index.
    Ready,
    /// Project is registered but has no `.julie/` directory yet (needs first index).
    Registered,
    /// Background indexing is in progress.
    Indexing,
    /// Workspace exists but may be outdated.
    Stale,
    /// Failed to load — error message included.
    Error(String),
}

/// A loaded workspace entry in the daemon.
#[derive(Clone)]
pub struct LoadedWorkspace {
    /// The loaded workspace with database and search index.
    pub workspace: JulieWorkspace,
    /// Current load status.
    pub status: WorkspaceLoadStatus,
    /// Project path on disk.
    pub path: PathBuf,
}

/// Daemon-wide shared state.
///
/// Holds all loaded workspaces and per-workspace MCP services.
/// Wrapped in `Arc<tokio::sync::RwLock<DaemonState>>` for shared access.
pub struct DaemonState {
    /// Map of workspace_id -> loaded workspace.
    pub workspaces: HashMap<String, LoadedWorkspace>,

    /// Per-workspace MCP services, keyed by workspace_id.
    /// Each workspace gets its own `StreamableHttpService` so sessions
    /// are isolated per-project.
    pub mcp_services: HashMap<String, StreamableHttpService<JulieServerHandler>>,

    /// Cross-project file watcher manager.
    ///
    /// Manages one `notify::RecommendedWatcher` per `Ready` project so the
    /// daemon detects file changes and triggers incremental re-indexing.
    pub watcher_manager: Arc<DaemonWatcherManager>,
}

impl DaemonState {
    /// Create a new empty daemon state.
    ///
    /// Initializes the cross-project file watcher manager. This should not fail
    /// unless the glob patterns in `watcher::filtering` are invalid (a bug).
    pub fn new() -> Self {
        let watcher_manager = Arc::new(
            DaemonWatcherManager::new().expect("Failed to build watcher ignore patterns (bug)")
        );

        Self {
            workspaces: HashMap::new(),
            mcp_services: HashMap::new(),
            watcher_manager,
        }
    }

    /// Load workspaces for all registered projects.
    ///
    /// For each project in the registry:
    /// - If it has a `.julie/` directory, try to load the workspace (detect_and_load).
    /// - If loading succeeds, mark as Ready and create an MCP service for it.
    /// - If it has no `.julie/` directory, mark as Registered.
    /// - If loading fails, mark as Error with the failure message.
    ///
    /// This method does NOT block on indexing. It only loads existing indexes.
    /// Projects that need indexing stay as `Registered` and can be indexed later
    /// by the background indexing pipeline (Task 9).
    pub async fn load_registered_projects(
        &mut self,
        registry: &GlobalRegistry,
        cancellation_token: &CancellationToken,
    ) {
        for (workspace_id, entry) in &registry.projects {
            let project_path = &entry.path;
            info!(
                "Loading workspace for project '{}' ({}): {}",
                entry.name,
                workspace_id,
                project_path.display()
            );

            let julie_dir = project_path.join(".julie");
            if !julie_dir.exists() || !julie_dir.is_dir() {
                info!(
                    "Project '{}' has no .julie directory — marking as Registered",
                    entry.name
                );
                self.workspaces.insert(
                    workspace_id.clone(),
                    LoadedWorkspace {
                        workspace: JulieWorkspace::empty_shell(project_path.clone()),
                        status: WorkspaceLoadStatus::Registered,
                        path: project_path.clone(),
                    },
                );
                continue;
            }

            match Self::try_load_workspace(project_path).await {
                Ok(workspace) => {
                    info!(
                        "Workspace for '{}' loaded successfully",
                        entry.name
                    );

                    // Determine status: Ready if we have both db and search_index
                    let status = if workspace.db.is_some() && workspace.search_index.is_some() {
                        WorkspaceLoadStatus::Ready
                    } else {
                        WorkspaceLoadStatus::Stale
                    };

                    // Create an MCP service for this workspace
                    let mcp_service = Self::create_workspace_mcp_service(
                        project_path.clone(),
                        cancellation_token,
                    );

                    self.workspaces.insert(
                        workspace_id.clone(),
                        LoadedWorkspace {
                            workspace,
                            status,
                            path: project_path.clone(),
                        },
                    );
                    self.mcp_services
                        .insert(workspace_id.clone(), mcp_service);
                }
                Err(e) => {
                    warn!(
                        "Failed to load workspace for '{}': {}",
                        entry.name, e
                    );
                    self.workspaces.insert(
                        workspace_id.clone(),
                        LoadedWorkspace {
                            workspace: JulieWorkspace::empty_shell(project_path.clone()),
                            status: WorkspaceLoadStatus::Error(e.to_string()),
                            path: project_path.clone(),
                        },
                    );
                }
            }
        }
    }

    /// Try to load an existing workspace from a project path.
    ///
    /// Uses `JulieWorkspace::detect_and_load` which searches for a `.julie`
    /// directory and initializes database + search index.
    async fn try_load_workspace(project_path: &Path) -> Result<JulieWorkspace> {
        JulieWorkspace::detect_and_load(project_path.to_path_buf())
            .await?
            .with_context(|| {
                format!(
                    "No workspace found at {}",
                    project_path.display()
                )
            })
    }

    /// Create an MCP service for a specific workspace.
    ///
    /// The handler factory closure creates a `JulieServerHandler` pointing
    /// at the workspace's project root, so when the handler initializes,
    /// it loads the correct workspace.
    pub fn create_workspace_mcp_service(
        workspace_root: PathBuf,
        cancellation_token: &CancellationToken,
    ) -> StreamableHttpService<JulieServerHandler> {
        let config = StreamableHttpServerConfig {
            cancellation_token: cancellation_token.clone(),
            ..Default::default()
        };
        let session_manager = Arc::new(LocalSessionManager::default());

        StreamableHttpService::new(
            move || {
                JulieServerHandler::new_sync(workspace_root.clone())
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
            },
            session_manager,
            config,
        )
    }

    /// Get the load status for a workspace, translated to a `ProjectStatus`
    /// suitable for API responses.
    pub fn project_status_for(&self, workspace_id: &str) -> ProjectStatus {
        match self.workspaces.get(workspace_id) {
            Some(loaded) => match &loaded.status {
                WorkspaceLoadStatus::Ready => ProjectStatus::Ready,
                WorkspaceLoadStatus::Registered => ProjectStatus::Registered,
                WorkspaceLoadStatus::Indexing => ProjectStatus::Indexing,
                WorkspaceLoadStatus::Stale => ProjectStatus::Stale,
                WorkspaceLoadStatus::Error(msg) => ProjectStatus::Error(msg.clone()),
            },
            None => ProjectStatus::Registered,
        }
    }

    /// Register a new workspace and create its MCP service.
    ///
    /// Called when a project is added via the API while the daemon is running.
    pub fn register_workspace(
        &mut self,
        workspace_id: String,
        project_path: PathBuf,
        cancellation_token: &CancellationToken,
    ) {
        debug!(
            "Registering new workspace {} at {}",
            workspace_id,
            project_path.display()
        );

        let julie_dir = project_path.join(".julie");
        let status = if julie_dir.exists() && julie_dir.is_dir() {
            // Has .julie dir but we haven't loaded it yet — mark stale
            // (the background indexer will pick it up)
            WorkspaceLoadStatus::Stale
        } else {
            WorkspaceLoadStatus::Registered
        };

        self.workspaces.insert(
            workspace_id.clone(),
            LoadedWorkspace {
                workspace: JulieWorkspace::empty_shell(project_path.clone()),
                status,
                path: project_path.clone(),
            },
        );

        // Always create an MCP service so the workspace is immediately
        // connectable (the handler will create the workspace on first use).
        let mcp_service = Self::create_workspace_mcp_service(
            project_path,
            cancellation_token,
        );
        self.mcp_services.insert(workspace_id, mcp_service);
    }

    /// Remove a workspace, its MCP service, and its file watcher.
    ///
    /// Called when a project is removed via the API.
    pub async fn remove_workspace(&mut self, workspace_id: &str) {
        self.workspaces.remove(workspace_id);
        self.mcp_services.remove(workspace_id);
        self.watcher_manager.stop_watching(workspace_id).await;
    }

    /// Start file watchers for all `Ready` projects.
    ///
    /// Called after `load_registered_projects` on daemon startup.
    /// Only starts watchers for workspaces that have both a database and
    /// search index loaded (status == Ready).
    pub async fn start_watchers_for_ready_projects(&self) {
        let mut started = 0u32;
        for (workspace_id, loaded) in &self.workspaces {
            if loaded.status != WorkspaceLoadStatus::Ready {
                continue;
            }

            let (db, search_index) = match (&loaded.workspace.db, &loaded.workspace.search_index) {
                (Some(db), si) => (db.clone(), si.clone()),
                _ => {
                    debug!(
                        "Skipping watcher for '{}': no database loaded",
                        workspace_id
                    );
                    continue;
                }
            };

            self.watcher_manager
                .start_watching(
                    workspace_id.clone(),
                    loaded.path.clone(),
                    db,
                    search_index,
                )
                .await;
            started += 1;
        }
        info!("Started file watchers for {} Ready project(s)", started);
    }

    /// Start a file watcher for a single workspace if it's Ready.
    ///
    /// Called after registering a new project via the API.
    pub async fn start_watcher_if_ready(&self, workspace_id: &str) {
        let loaded = match self.workspaces.get(workspace_id) {
            Some(lw) => lw,
            None => return,
        };

        if loaded.status != WorkspaceLoadStatus::Ready {
            debug!(
                "Not starting watcher for '{}': status is {:?}",
                workspace_id, loaded.status
            );
            return;
        }

        let (db, search_index) = match (&loaded.workspace.db, &loaded.workspace.search_index) {
            (Some(db), si) => (db.clone(), si.clone()),
            _ => return,
        };

        self.watcher_manager
            .start_watching(
                workspace_id.to_string(),
                loaded.path.clone(),
                db,
                search_index,
            )
            .await;
    }
}
