use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::daemon::database::DaemonDatabase;
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::watcher_pool::WatcherPool;
use crate::tools::workspace::indexing::state::IndexingRuntimeSnapshot;
use crate::workspace::JulieWorkspace;

/// A pool of shared `JulieWorkspace` instances for the daemon.
///
/// Multiple MCP sessions connecting to the same project share a single workspace
/// (database, search index) rather than each session initializing its own copy.
/// Indexes are stored under a shared directory (typically `~/.julie/indexes/`)
/// rather than per-project `.julie/indexes/`.
pub struct WorkspacePool {
    workspaces: tokio::sync::RwLock<HashMap<String, WorkspaceEntry>>,
    indexes_dir: PathBuf,
    daemon_db: Option<Arc<DaemonDatabase>>,
}

struct WorkspaceEntry {
    workspace: Arc<JulieWorkspace>,
}

impl WorkspacePool {
    /// Create an empty workspace pool.
    ///
    /// `indexes_dir` is the shared root for all workspace indexes,
    /// typically `~/.julie/indexes/`.
    ///
    /// `daemon_db` is the persistent registry database. When `Some`, workspace
    /// state (status, session counts) is persisted across daemon restarts.
    ///
    /// `_embedding_service` is accepted temporarily while callers migrate to
    /// `WorkspaceSessionAttachment`.
    pub fn new(
        indexes_dir: PathBuf,
        daemon_db: Option<Arc<DaemonDatabase>>,
        _watcher_pool: Option<Arc<WatcherPool>>,
        _embedding_service: Option<Arc<EmbeddingService>>,
    ) -> Self {
        Self {
            workspaces: tokio::sync::RwLock::new(HashMap::new()),
            indexes_dir,
            daemon_db,
        }
    }

    /// Get an existing workspace without initializing.
    /// Returns `None` if the workspace hasn't been initialized yet.
    pub async fn get(&self, workspace_id: &str) -> Option<Arc<JulieWorkspace>> {
        let guard = self.workspaces.read().await;
        guard.get(workspace_id).map(|e| Arc::clone(&e.workspace))
    }

    /// Get an existing workspace or initialize a new one.
    ///
    /// Uses double-checked locking: takes a read lock first (fast path),
    /// then upgrades to a write lock only when initialization is needed.
    ///
    /// When `daemon_db` is present, the workspace is registered as `pending`.
    /// Session counts and watcher refs are mutated by
    /// `WorkspaceSessionAttachment`, not by runtime lookup.
    pub async fn get_or_init(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<Arc<JulieWorkspace>> {
        // Fast path: read lock (drop before any async work to avoid holding across awaits)
        let cached_ws = {
            let guard = self.workspaces.read().await;
            guard.get(workspace_id).map(|e| Arc::clone(&e.workspace))
        };

        if let Some(ws) = cached_ws {
            return Ok(ws);
        }

        // Slow path: write lock + initialization
        let mut guard = self.workspaces.write().await;

        // Double-check: another task may have initialized while we waited for the write lock
        if let Some(entry) = guard.get(workspace_id) {
            let ws = Arc::clone(&entry.workspace);
            return Ok(ws);
        }

        info!(
            workspace_id = workspace_id,
            root = %workspace_root.display(),
            "Initializing workspace in pool"
        );

        if !workspace_root.exists() {
            anyhow::bail!(
                "Workspace path does not exist: {}",
                workspace_root.display()
            );
        }
        if !workspace_root.is_dir() {
            anyhow::bail!(
                "Workspace path is not a directory: {}",
                workspace_root.display()
            );
        }

        if let Some(julie_home) = self.indexes_dir.parent() {
            let daemon_paths = crate::paths::DaemonPaths::with_home(julie_home.to_path_buf());
            if let Err(e) = crate::migration::run_migration_for_workspace(
                &daemon_paths,
                &workspace_root,
                self.daemon_db.clone(),
            ) {
                warn!(
                    workspace_id,
                    root = %workspace_root.display(),
                    "Failed to reconcile per-project indexes before pool init: {e:#}"
                );
            }
        } else {
            warn!(
                workspace_id,
                indexes_dir = %self.indexes_dir.display(),
                "WorkspacePool indexes_dir has no julie_home parent, skipping migration pass"
            );
        }

        // Register as pending so the workspace is visible in daemon.db even
        // while initializing. Runtime lookup does not attach a session.
        if let Some(ref db) = self.daemon_db {
            let path_str = workspace_root.to_string_lossy();
            if let Err(e) = db.upsert_workspace(workspace_id, &path_str, "pending") {
                warn!(
                    workspace_id,
                    path = %path_str,
                    "Failed to register workspace in daemon.db: {}", e
                );
            }
        }

        let workspace = self
            .init_workspace(workspace_id, workspace_root)
            .await
            .with_context(|| format!("Failed to initialize workspace '{workspace_id}' in pool"))?;

        let ws = Arc::new(workspace);
        guard.insert(
            workspace_id.to_string(),
            WorkspaceEntry {
                workspace: Arc::clone(&ws),
            },
        );
        Ok(ws)
    }

    /// Number of active workspaces in the pool.
    pub async fn active_count(&self) -> usize {
        let guard = self.workspaces.read().await;
        guard.len()
    }

    pub fn indexes_dir(&self) -> &std::path::Path {
        &self.indexes_dir
    }

    pub(crate) async fn indexing_snapshot(
        &self,
        workspace_id: &str,
    ) -> Option<IndexingRuntimeSnapshot> {
        let guard = self.workspaces.read().await;
        guard.get(workspace_id).map(|entry| {
            entry
                .workspace
                .indexing_runtime
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .snapshot()
        })
    }

    pub async fn evict_workspace(&self, workspace_id: &str) -> bool {
        let mut guard = self.workspaces.write().await;
        guard.remove(workspace_id).is_some()
    }

    pub(crate) async fn indexing_snapshots(
        &self,
    ) -> Vec<(
        String,
        crate::tools::workspace::indexing::state::IndexingRuntimeSnapshot,
    )> {
        let guard = self.workspaces.read().await;
        guard
            .iter()
            .map(|(workspace_id, entry)| {
                let snapshot = entry
                    .workspace
                    .indexing_runtime
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .snapshot();
                (workspace_id.clone(), snapshot)
            })
            .collect()
    }

    pub(crate) async fn projection_inputs(
        &self,
    ) -> Vec<(
        String,
        Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
        bool,
    )> {
        let guard = self.workspaces.read().await;
        let mut inputs: Vec<_> = guard
            .iter()
            .filter_map(|(workspace_id, entry)| {
                entry.workspace.db.as_ref().map(|db| {
                    (
                        workspace_id.clone(),
                        Arc::clone(db),
                        entry.workspace.search_index.is_some(),
                    )
                })
            })
            .collect();
        inputs.sort_by(|left, right| left.0.cmp(&right.0));
        inputs
    }

    /// Initialize a `JulieWorkspace` with its index root redirected to the pool's
    /// shared indexes directory.
    async fn init_workspace(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<JulieWorkspace> {
        let index_root = self.indexes_dir.join(workspace_id);

        // Create a workspace with the standard initializer, then redirect its
        // index root before initializing db/search. We build the workspace
        // manually to avoid the full `JulieWorkspace::initialize` which creates
        // folder structure and config under .julie (the daemon may not own that).
        let julie_dir = workspace_root.join(".julie");
        std::fs::create_dir_all(&julie_dir)
            .with_context(|| format!("Failed to create .julie dir at {}", julie_dir.display()))?;

        let mut workspace = JulieWorkspace {
            root: workspace_root,
            julie_dir,
            db: None,
            search_index: None,
            watcher: None,
            embedding_provider: None,
            embedding_runtime_status: None,
            config: Default::default(),
            index_root_override: Some(index_root),
            indexing_runtime:
                crate::tools::workspace::indexing::state::IndexingRuntimeState::shared(),
        };

        // Initialize database and search index (they use indexes_root_path(),
        // which now points to the pool's shared directory).
        workspace.initialize_database()?;
        workspace.initialize_search_index()?;

        Ok(workspace)
    }
}
