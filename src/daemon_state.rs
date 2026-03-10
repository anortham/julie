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
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn, debug};

use crate::daemon_indexer::{IndexRequest, IndexingSender};
use crate::daemon_watcher::DaemonWatcherManager;
use crate::handler::JulieServerHandler;
use crate::registry::{GlobalRegistry, ProjectStatus};
use crate::workspace::JulieWorkspace;

/// Result of a successful project registration via `DaemonState::register_project`.
#[derive(Debug, Clone)]
pub struct ProjectRegistrationResult {
    /// The workspace ID (e.g. "myproject_a1b2c3d4").
    pub workspace_id: String,
    /// Human-readable project name (derived from directory name).
    pub name: String,
    /// Canonical absolute path to the project directory.
    pub path: PathBuf,
    /// Whether the project was already registered (true) or newly created (false).
    pub already_existed: bool,
}

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

/// Daemon-wide shared state: loaded workspaces, MCP services, and daemon-level
/// resources. Wrapped in `Arc<tokio::sync::RwLock<DaemonState>>` for shared access.
pub struct DaemonState {
    /// Map of workspace_id -> loaded workspace.
    pub(crate) workspaces: HashMap<String, LoadedWorkspace>,
    /// Per-workspace MCP services (isolated per-project sessions).
    pub(crate) mcp_services: HashMap<String, StreamableHttpService<JulieServerHandler>>,
    /// Cross-project file watcher manager (one watcher per Ready project).
    pub(crate) watcher_manager: Arc<DaemonWatcherManager>,
    /// Global project registry — shared with `AppState`.
    pub(crate) registry: Arc<RwLock<GlobalRegistry>>,
    /// Path to `~/.julie` for persisting registry.
    pub(crate) julie_home: PathBuf,
    /// Sender for background indexing pipeline (`None` during construction;
    /// set via `set_indexing_sender` after spawning the worker).
    pub(crate) indexing_sender: Option<IndexingSender>,
    /// Cancellation token for shutting down all MCP sessions and background work.
    pub(crate) cancellation_token: CancellationToken,
}

impl DaemonState {
    /// Create a new empty daemon state. Call `set_indexing_sender` after
    /// spawning the indexing worker (chicken-and-egg dependency).
    pub fn new(
        registry: Arc<RwLock<GlobalRegistry>>,
        julie_home: PathBuf,
        cancellation_token: CancellationToken,
    ) -> Self {
        let watcher_manager = Arc::new(
            DaemonWatcherManager::new().expect("Failed to build watcher ignore patterns (bug)")
        );

        Self {
            workspaces: HashMap::new(),
            mcp_services: HashMap::new(),
            watcher_manager,
            registry,
            julie_home,
            indexing_sender: None,
            cancellation_token,
        }
    }

    /// Set the indexing sender after the indexing worker has been spawned.
    pub fn set_indexing_sender(&mut self, sender: IndexingSender) {
        self.indexing_sender = Some(sender);
    }

    /// Collect workspace IDs and paths for workspaces that need indexing.
    ///
    /// Returns entries with `Registered` or `Stale` status — these are projects
    /// that were loaded on startup but don't yet have a complete index.
    pub fn workspaces_needing_indexing(&self) -> Vec<(String, PathBuf)> {
        self.workspaces
            .iter()
            .filter(|(_, w)| {
                matches!(
                    w.status,
                    WorkspaceLoadStatus::Registered | WorkspaceLoadStatus::Stale
                )
            })
            .map(|(id, w)| (id.clone(), w.path.clone()))
            .collect()
    }

    /// Register a project: validate path, register in GlobalRegistry, create
    /// workspace + MCP service, persist registry, start watcher, queue indexing.
    ///
    /// Single source of truth for registration (used by both API and MCP tool).
    /// Returns `already_existed: true` if the project was already registered.
    pub async fn register_project(
        daemon_state: &Arc<RwLock<DaemonState>>,
        path: &Path,
    ) -> Result<ProjectRegistrationResult> {
        // Step 1: Validate path exists and is a directory
        if !path.exists() {
            anyhow::bail!("Path does not exist: {}", path.display());
        }
        if !path.is_dir() {
            anyhow::bail!("Path is not a directory: {}", path.display());
        }

        // Step 2: Register in GlobalRegistry
        //
        // LOCK ORDERING: We must NOT hold DaemonState while acquiring Registry,
        // because daemon_indexer acquires Registry then DaemonState (ABBA deadlock).
        // Instead: read DaemonState briefly to get the registry Arc, drop the
        // DaemonState lock, then acquire registry independently.
        let (registry_arc, julie_home) = {
            let ds = daemon_state.read().await;
            (ds.registry.clone(), ds.julie_home.clone())
        };
        // DaemonState lock is now released — safe to acquire registry
        let (workspace_id, name, canonical_path, is_new) = {
            let mut registry = registry_arc.write().await;
            let result = registry.register_project(path)?;
            let wid = result.workspace_id().to_string();
            let entry = registry.get_project(&wid).unwrap();
            let name = entry.name.clone();
            let cpath = entry.path.clone();
            let is_new = !result.is_already_exists();

            if is_new {
                // Step 3: Persist registry to disk
                registry.save(&julie_home).map_err(|e| {
                    tracing::error!("Failed to save registry after adding project: {}", e);
                    anyhow::anyhow!(
                        "Project registered in memory but registry file write failed: {}",
                        e
                    )
                })?;
            }

            (wid, name, cpath, is_new)
        };

        if !is_new {
            return Ok(ProjectRegistrationResult {
                workspace_id,
                name,
                path: canonical_path,
                already_existed: true,
            });
        }

        // Step 4: Register workspace in DaemonState (creates handler + MCP service)
        {
            let mut ds = daemon_state.write().await;
            ds.register_workspace(
                workspace_id.clone(),
                canonical_path.clone(),
                daemon_state.clone(),
            );
        }

        // Step 5: Start file watcher if ready (read lock only)
        {
            let ds = daemon_state.read().await;
            ds.start_watcher_if_ready(&workspace_id).await;
        }

        // Step 6: Queue background indexing
        {
            let ds = daemon_state.read().await;
            if let Some(sender) = &ds.indexing_sender {
                let index_request = IndexRequest {
                    workspace_id: workspace_id.clone(),
                    project_path: canonical_path.clone(),
                    force: false,
                };
                if let Err(e) = sender.send(index_request).await {
                    tracing::warn!("Failed to queue auto-indexing for new project: {}", e);
                }
            } else {
                tracing::warn!(
                    "No indexing sender available — project '{}' registered but indexing not queued",
                    workspace_id
                );
            }
        }

        Ok(ProjectRegistrationResult {
            workspace_id,
            name,
            path: canonical_path,
            already_existed: false,
        })
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
        daemon_state: Arc<RwLock<DaemonState>>,
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
                // Always create MCP service so /mcp/{workspace_id} is reachable.
                // The handler will initialize the workspace on first use.
                let mcp_service = Self::create_workspace_mcp_service(
                    project_path.clone(),
                    &self.cancellation_token,
                    daemon_state.clone(),
                );
                self.mcp_services
                    .insert(workspace_id.clone(), mcp_service);
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
                        &self.cancellation_token,
                        daemon_state.clone(),
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
                    // Create MCP service even on error so the workspace is
                    // reachable — handler can retry initialization on connect.
                    let mcp_service = Self::create_workspace_mcp_service(
                        project_path.clone(),
                        &self.cancellation_token,
                        daemon_state.clone(),
                    );
                    self.mcp_services
                        .insert(workspace_id.clone(), mcp_service);
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
        daemon_state: Arc<RwLock<DaemonState>>,
    ) -> StreamableHttpService<JulieServerHandler> {
        let config = StreamableHttpServerConfig {
            cancellation_token: cancellation_token.clone(),
            ..Default::default()
        };
        let session_manager = Arc::new(LocalSessionManager::default());

        StreamableHttpService::new(
            move || {
                JulieServerHandler::new_with_daemon_state(
                    workspace_root.clone(),
                    daemon_state.clone(),
                )
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
        daemon_state: Arc<RwLock<DaemonState>>,
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
            &self.cancellation_token,
            daemon_state,
        );
        self.mcp_services.insert(workspace_id, mcp_service);
    }

    /// Remove a workspace, its MCP service, and its file watcher.
    ///
    /// Low-level removal — called by `deregister_project`. Prefer that method
    /// for full deregistration (which also handles GlobalRegistry + disk persist).
    pub async fn remove_workspace(&mut self, workspace_id: &str) {
        self.workspaces.remove(workspace_id);
        self.mcp_services.remove(workspace_id);
        self.watcher_manager.stop_watching(workspace_id).await;
    }

    // NOTE: Watcher methods (`start_watchers_for_ready_projects`,
    // `start_watcher_if_ready`) are in `daemon_state_watchers.rs` to keep
    // this file under the 500-line limit.
}

/// Result of a successful project deregistration via `DaemonState::deregister_project`.
#[derive(Debug, Clone)]
pub struct DeregistrationResult {
    /// The workspace ID that was removed.
    pub workspace_id: String,
    /// Human-readable project name (derived from directory name).
    pub name: String,
    /// Path to the project directory.
    pub path: PathBuf,
}

impl DaemonState {
    /// Deregister a project from the daemon: stops file watcher, removes from
    /// DaemonState (workspaces + mcp_services), deregisters from GlobalRegistry,
    /// and persists the registry to disk.
    ///
    /// This is the single source of truth for project deregistration logic.
    /// Both `DELETE /api/projects/:id` and the MCP `manage_workspace remove` tool
    /// call this in daemon mode.
    ///
    /// Takes the `Arc<RwLock<DaemonState>>` so it can acquire/release locks
    /// with correct ordering (DaemonState before registry).
    ///
    /// Returns `Ok(Some(DeregistrationResult))` on success, or `Ok(None)` if the
    /// workspace_id was not found in the registry.
    pub async fn deregister_project(
        daemon_state: &Arc<RwLock<DaemonState>>,
        workspace_id: &str,
    ) -> Result<Option<DeregistrationResult>> {
        // Step 1: Acquire DaemonState write lock, then registry write lock
        // (correct ordering: DaemonState before registry).
        let result = {
            let mut ds = daemon_state.write().await;

            // Check if the project exists in the registry
            let (name, path) = {
                let mut registry = ds.registry.write().await;

                let entry = match registry.get_project(workspace_id) {
                    Some(e) => e,
                    None => return Ok(None),
                };
                let name = entry.name.clone();
                let path = entry.path.clone();

                // Remove from GlobalRegistry
                registry.remove_project(workspace_id);

                // Persist registry to disk
                let julie_home = ds.julie_home.clone();
                registry.save(&julie_home).map_err(|e| {
                    tracing::error!(
                        "Failed to save registry after removing project '{}': {}",
                        workspace_id,
                        e
                    );
                    anyhow::anyhow!(
                        "Project deregistered in memory but registry file write failed: {}",
                        e
                    )
                })?;

                (name, path)
            };
            // Registry lock dropped here

            // Step 2: Remove from DaemonState (workspace + MCP service + watcher)
            ds.remove_workspace(workspace_id).await;

            DeregistrationResult {
                workspace_id: workspace_id.to_string(),
                name,
                path,
            }
        };
        // DaemonState lock dropped here

        Ok(Some(result))
    }
}
