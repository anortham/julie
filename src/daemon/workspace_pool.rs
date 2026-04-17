use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::daemon::database::DaemonDatabase;
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::watcher_pool::WatcherPool;
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
    watcher_pool: Option<Arc<WatcherPool>>,
    embedding_service: Option<Arc<EmbeddingService>>,
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
    ///
    /// `daemon_db` is the persistent registry database. When `Some`, workspace
    /// state (status, session counts) is persisted across daemon restarts.
    ///
    /// `watcher_pool` is the shared file watcher registry. When `Some`,
    /// `IncrementalIndexer` instances are ref-counted across sessions so that
    /// each workspace has exactly one active file watcher regardless of how
    /// many sessions are attached.
    ///
    /// `embedding_service` is the shared embedding provider, passed through
    /// to `WatcherPool::attach()` so incremental re-indexing can re-embed
    /// changed symbols.
    pub fn new(
        indexes_dir: PathBuf,
        daemon_db: Option<Arc<DaemonDatabase>>,
        watcher_pool: Option<Arc<WatcherPool>>,
        embedding_service: Option<Arc<EmbeddingService>>,
    ) -> Self {
        Self {
            workspaces: tokio::sync::RwLock::new(HashMap::new()),
            indexes_dir,
            daemon_db,
            watcher_pool,
            embedding_service,
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
    /// When `daemon_db` is present, the workspace is registered as `pending` and
    /// its session count is incremented.
    ///
    /// When `watcher_pool` is present, `attach()` is called to increment the
    /// watcher ref-count (and start a new `IncrementalIndexer` if needed).
    /// Watcher failures are non-fatal: a warning is logged and initialization
    /// continues, since file watching is a convenience, not a hard requirement.
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
            self.update_session_count(workspace_id, true).await;
            if let Some(ref wp) = self.watcher_pool {
                let provider = self.shared_embedding_provider();
                if let Err(e) = wp.attach(workspace_id, &ws, provider).await {
                    warn!(
                        workspace_id,
                        "Failed to attach watcher on session reuse: {}", e
                    );
                }
            }
            return Ok(ws);
        }

        // Slow path: write lock + initialization
        let mut guard = self.workspaces.write().await;

        // Double-check: another task may have initialized while we waited for the write lock
        if let Some(entry) = guard.get(workspace_id) {
            let ws = Arc::clone(&entry.workspace);
            drop(guard);
            self.update_session_count(workspace_id, true).await;
            if let Some(ref wp) = self.watcher_pool {
                let provider = self.shared_embedding_provider();
                if let Err(e) = wp.attach(workspace_id, &ws, provider).await {
                    warn!(
                        workspace_id,
                        "Failed to attach watcher (double-check path): {}", e
                    );
                }
            }
            return Ok(ws);
        }

        info!(
            workspace_id = workspace_id,
            root = %workspace_root.display(),
            "Initializing workspace in pool"
        );

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

        // Register as pending so the workspace is visible in daemon.db even while
        // initializing. The session count is incremented AFTER a successful init
        // to avoid a permanently-leaked count if init_workspace fails.
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
                indexed: false,
            },
        );
        drop(guard); // release write lock before any async follow-up work

        // Increment only after successful init — safe to count now.
        self.update_session_count(workspace_id, true).await;

        if let Some(ref wp) = self.watcher_pool {
            let provider = self.shared_embedding_provider();
            if let Err(e) = wp.attach(workspace_id, &ws, provider).await {
                warn!(workspace_id, "Failed to attach watcher: {}", e);
            }
        }

        Ok(ws)
    }

    /// Check whether a workspace has completed its initial indexing pass.
    pub async fn is_indexed(&self, workspace_id: &str) -> bool {
        let guard = self.workspaces.read().await;
        guard.get(workspace_id).is_some_and(|entry| entry.indexed)
    }

    /// Mark a workspace as having completed its initial indexing pass.
    ///
    /// Also updates daemon.db status to "ready".
    pub async fn mark_indexed(&self, workspace_id: &str) {
        let mut guard = self.workspaces.write().await;
        if let Some(entry) = guard.get_mut(workspace_id) {
            entry.indexed = true;
        }
        if let Some(ref db) = self.daemon_db {
            let _ = db.update_workspace_status(workspace_id, "ready");
        }
    }

    /// Sync the pool's in-memory `indexed` flag from daemon.db.
    ///
    /// Called at IPC session tear-down: if daemon.db records the workspace as
    /// "ready" (set by `handle_index_command` after a successful indexing pass),
    /// the pool's in-memory `indexed` flag is set to match. This ensures
    /// `is_indexed()` returns the correct value for subsequent sessions and
    /// that the pool state stays consistent with the persistent registry.
    pub async fn sync_indexed_from_db(&self, workspace_id: &str) {
        let Some(ref db) = self.daemon_db else { return };
        if let Ok(Some(row)) = db.get_workspace(workspace_id) {
            if row.status == "ready" {
                let mut guard = self.workspaces.write().await;
                if let Some(entry) = guard.get_mut(workspace_id) {
                    entry.indexed = true;
                }
            }
        }
    }

    /// Decrement session count and watcher ref when a session disconnects.
    ///
    /// Session count is clamped to 0 (never goes negative). Watcher ref
    /// decrement starts the grace period when the last session disconnects.
    pub async fn disconnect_session(&self, workspace_id: &str) {
        self.update_session_count(workspace_id, false).await;
        if let Some(ref wp) = self.watcher_pool {
            wp.detach(workspace_id).await;
        }
    }

    async fn update_session_count(&self, workspace_id: &str, increment: bool) {
        let Some(db) = self.daemon_db.as_ref().map(Arc::clone) else {
            return;
        };
        let workspace_id = workspace_id.to_string();
        let workspace_id_for_log = workspace_id.clone();
        let op = if increment { "increment" } else { "decrement" };

        let result = tokio::task::spawn_blocking(move || {
            if increment {
                db.increment_session_count(&workspace_id)
            } else {
                db.decrement_session_count(&workspace_id)
            }
        })
        .await;

        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                warn!(
                    workspace_id = workspace_id_for_log,
                    "Failed to {op} workspace session count in daemon.db: {e}"
                );
            }
            Err(e) => {
                warn!(
                    workspace_id = workspace_id_for_log,
                    "Failed to run session count {op} in background task: {e}"
                );
            }
        }
    }

    /// Number of active workspaces in the pool.
    pub async fn active_count(&self) -> usize {
        let guard = self.workspaces.read().await;
        guard.len()
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

    /// Extract the shared embedding provider from the service (if available).
    ///
    /// Returns `None` when no embedding service is configured or when the
    /// service initialized without a provider (e.g., model download failed).
    fn shared_embedding_provider(&self) -> Option<Arc<dyn crate::embeddings::EmbeddingProvider>> {
        self.embedding_service
            .as_ref()
            .and_then(|svc| svc.provider())
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
