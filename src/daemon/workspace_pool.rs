use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::info;

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
}

struct WorkspaceEntry {
    workspace: Arc<JulieWorkspace>,
    indexed: bool,
}

impl WorkspacePool {
    /// Create an empty workspace pool.
    ///
    /// `indexes_dir` is the shared root for all workspace indexes,
    /// typically `~/.julie/indexes/`.
    pub fn new(indexes_dir: PathBuf) -> Self {
        Self {
            workspaces: tokio::sync::RwLock::new(HashMap::new()),
            indexes_dir,
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
    pub async fn get_or_init(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<Arc<JulieWorkspace>> {
        // Fast path: read lock
        {
            let guard = self.workspaces.read().await;
            if let Some(entry) = guard.get(workspace_id) {
                return Ok(Arc::clone(&entry.workspace));
            }
        }

        // Slow path: write lock + initialization
        let mut guard = self.workspaces.write().await;

        // Double-check: another task may have initialized while we waited for the write lock
        if let Some(entry) = guard.get(workspace_id) {
            return Ok(Arc::clone(&entry.workspace));
        }

        info!(
            workspace_id = workspace_id,
            root = %workspace_root.display(),
            "Initializing workspace in pool"
        );

        let workspace = self
            .init_workspace(workspace_id, workspace_root)
            .await
            .with_context(|| {
                format!("Failed to initialize workspace '{workspace_id}' in pool")
            })?;

        let ws = Arc::new(workspace);
        guard.insert(
            workspace_id.to_string(),
            WorkspaceEntry {
                workspace: Arc::clone(&ws),
                indexed: false,
            },
        );

        Ok(ws)
    }

    /// Check whether a workspace has completed its initial indexing pass.
    pub async fn is_indexed(&self, workspace_id: &str) -> bool {
        let guard = self.workspaces.read().await;
        guard
            .get(workspace_id)
            .is_some_and(|entry| entry.indexed)
    }

    /// Mark a workspace as having completed its initial indexing pass.
    pub async fn mark_indexed(&self, workspace_id: &str) {
        let mut guard = self.workspaces.write().await;
        if let Some(entry) = guard.get_mut(workspace_id) {
            entry.indexed = true;
        }
    }

    /// Number of active workspaces in the pool.
    pub async fn active_count(&self) -> usize {
        let guard = self.workspaces.read().await;
        guard.len()
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
        };

        // Initialize database and search index (they use indexes_root_path(),
        // which now points to the pool's shared directory).
        workspace.initialize_database()?;
        workspace.initialize_search_index()?;

        Ok(workspace)
    }
}
