use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::daemon::connection_pool::WorkspaceConnectionPool;
use crate::daemon::database::DaemonDatabase;
use crate::daemon::watcher_pool::WatcherPool;
use crate::tools::workspace::indexing::state::IndexingRuntimeSnapshot;
use crate::workspace::JulieWorkspace;

/// Default idle threshold before an unused workspace is evicted from the pool.
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300;
const MIN_IDLE_TIMEOUT_SECS: u64 = 60;
const MAX_IDLE_TIMEOUT_SECS: u64 = 3600;
const IDLE_TIMEOUT_ENV: &str = "JULIE_WORKSPACE_IDLE_TIMEOUT_SECS";

/// Read the configured idle-workspace eviction threshold from the environment.
pub fn idle_timeout() -> Duration {
    match std::env::var(IDLE_TIMEOUT_ENV) {
        Ok(s) => match s.parse::<u64>() {
            Ok(v) if (MIN_IDLE_TIMEOUT_SECS..=MAX_IDLE_TIMEOUT_SECS).contains(&v) => {
                Duration::from_secs(v)
            }
            Ok(v) => {
                warn!(
                    value = v,
                    "JULIE_WORKSPACE_IDLE_TIMEOUT_SECS out of range [{},{}]; using default {}s",
                    MIN_IDLE_TIMEOUT_SECS,
                    MAX_IDLE_TIMEOUT_SECS,
                    DEFAULT_IDLE_TIMEOUT_SECS
                );
                Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "JULIE_WORKSPACE_IDLE_TIMEOUT_SECS unparseable; using default {}s",
                    DEFAULT_IDLE_TIMEOUT_SECS
                );
                Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS)
            }
        },
        Err(_) => Duration::from_secs(DEFAULT_IDLE_TIMEOUT_SECS),
    }
}

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
    project_local_julie_dir: bool,
}

struct WorkspaceEntry {
    workspace: Arc<JulieWorkspace>,
    last_accessed: StdMutex<Instant>,
    connection_pool: Arc<WorkspaceConnectionPool>,
}

impl WorkspaceEntry {
    fn new(workspace: Arc<JulieWorkspace>, connection_pool: Arc<WorkspaceConnectionPool>) -> Self {
        Self {
            workspace,
            last_accessed: StdMutex::new(Instant::now()),
            connection_pool,
        }
    }

    fn touch(&self) {
        let mut guard = self
            .last_accessed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Instant::now();
    }

    fn last_accessed(&self) -> Instant {
        *self
            .last_accessed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl WorkspacePool {
    /// Create an empty workspace pool.
    ///
    /// `indexes_dir` is the shared root for all workspace indexes,
    /// typically `~/.julie/indexes/`.
    ///
    /// `daemon_db` is the persistent registry database. When `Some`, workspace
    /// state (status, session counts) is persisted across daemon restarts.
    pub fn new(indexes_dir: PathBuf, daemon_db: Option<Arc<DaemonDatabase>>) -> Self {
        Self {
            workspaces: tokio::sync::RwLock::new(HashMap::new()),
            indexes_dir,
            daemon_db,
            project_local_julie_dir: true,
        }
    }

    /// Create a workspace pool that keeps all runtime state under `indexes_dir`.
    ///
    /// This is for certification and replay jobs that index external repos and
    /// must not create `.julie` directories in those repos.
    pub fn new_isolated(indexes_dir: PathBuf, daemon_db: Option<Arc<DaemonDatabase>>) -> Self {
        Self {
            workspaces: tokio::sync::RwLock::new(HashMap::new()),
            indexes_dir,
            daemon_db,
            project_local_julie_dir: false,
        }
    }

    /// Get an existing workspace without initializing.
    /// Returns `None` if the workspace hasn't been initialized yet.
    /// Refreshes the entry's `last_accessed` timestamp on a cache hit so the
    /// idle sweeper does not evict an actively-used workspace.
    pub async fn get(&self, workspace_id: &str) -> Option<Arc<JulieWorkspace>> {
        let guard = self.workspaces.read().await;
        let entry = guard.get(workspace_id)?;
        entry.touch();
        Some(Arc::clone(&entry.workspace))
    }

    /// Return the `WorkspaceConnectionPool` for an already-initialized workspace,
    /// or `None` if the workspace hasn't been loaded yet.
    pub async fn connection_pool(
        &self,
        workspace_id: &str,
    ) -> Option<Arc<WorkspaceConnectionPool>> {
        let map = self.workspaces.read().await;
        map.get(workspace_id)
            .map(|entry| Arc::clone(&entry.connection_pool))
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
            guard.get(workspace_id).map(|e| {
                e.touch();
                Arc::clone(&e.workspace)
            })
        };

        if let Some(ws) = cached_ws {
            return Ok(ws);
        }

        // Slow path: write lock + initialization
        let mut guard = self.workspaces.write().await;

        // Double-check: another task may have initialized while we waited for the write lock
        if let Some(entry) = guard.get(workspace_id) {
            entry.touch();
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
        crate::workspace::root_safety::reject_sensitive_workspace_root(&workspace_root)?;

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

        // Build a connection pool using the exact path the database was opened
        // at.  We read it from the initialized SymbolDatabase rather than
        // calling workspace.db_path(), because in daemon mode with an
        // index_root_override the actual db lives under the shared indexes dir
        // (workspace_db_path), not under julie_dir (db_path).
        let actual_db_path = workspace
            .db
            .as_ref()
            .map(|db| db.lock().unwrap_or_else(|p| p.into_inner()).file_path.clone())
            .with_context(|| {
                format!(
                    "Workspace '{workspace_id}' has no database after initialize_database()"
                )
            })?;
        let conn_pool = WorkspaceConnectionPool::new(actual_db_path)
            .with_context(|| {
                format!(
                    "Failed to create connection pool for workspace '{workspace_id}'"
                )
            })?;

        let ws = Arc::new(workspace);
        guard.insert(
            workspace_id.to_string(),
            WorkspaceEntry::new(Arc::clone(&ws), Arc::new(conn_pool)),
        );
        Ok(ws)
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

    /// Evict a single workspace from the pool, releasing its Tantivy file
    /// locks. Removes the entry under the write lock first, then calls
    /// `SearchIndex::shutdown()` on the removed entry outside the lock to
    /// avoid a concurrent `get()` returning a half-shutdown workspace.
    ///
    /// Returns `true` when an entry was removed, `false` when no such
    /// workspace was tracked.
    pub async fn evict_workspace(&self, workspace_id: &str) -> bool {
        let entry = {
            let mut guard = self.workspaces.write().await;
            guard.remove(workspace_id)
        };
        let Some(entry) = entry else {
            return false;
        };
        shutdown_workspace_entry(workspace_id, entry).await;
        true
    }

    /// Sweep the pool for entries idle longer than `idle_threshold` and evict
    /// them. Before removing each candidate, asks `watcher_pool` to drop the
    /// watcher; if the watcher still has refs (an attached session) the
    /// workspace is preserved.
    ///
    /// Returns the list of evicted workspace IDs.
    pub async fn sweep_idle_workspaces(
        &self,
        watcher_pool: &WatcherPool,
        idle_threshold: Duration,
    ) -> Vec<String> {
        // Collect candidates under a read lock.
        let now = Instant::now();
        let candidates: Vec<String> = {
            let guard = self.workspaces.read().await;
            guard
                .iter()
                .filter_map(|(id, entry)| {
                    let age = now.saturating_duration_since(entry.last_accessed());
                    if age >= idle_threshold {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        if candidates.is_empty() {
            return Vec::new();
        }

        let mut evicted = Vec::new();
        for workspace_id in candidates {
            // Watcher cleanup first: if the watcher still has refs (sessions
            // attached), skip this workspace entirely.
            let watcher_released = match watcher_pool.remove_if_inactive(&workspace_id).await {
                Ok(released) => released,
                Err(e) => {
                    warn!(
                        workspace_id = %workspace_id,
                        "Idle sweep: failed to release watcher: {e:#}"
                    );
                    continue;
                }
            };
            if !watcher_released {
                continue;
            }

            // Take a brief write lock, re-check age (a concurrent `get()` may
            // have refreshed the timestamp), and remove the entry.
            let entry = {
                let mut guard = self.workspaces.write().await;
                let Some(entry) = guard.get(&workspace_id) else {
                    continue;
                };
                let age = Instant::now().saturating_duration_since(entry.last_accessed());
                if age < idle_threshold {
                    continue;
                }
                guard.remove(&workspace_id)
            };
            let Some(entry) = entry else {
                continue;
            };
            shutdown_workspace_entry(&workspace_id, entry).await;
            info!(workspace_id = %workspace_id, "Evicted idle workspace");
            evicted.push(workspace_id);
        }

        evicted
    }

    /// Spawn a periodic background task that sweeps idle workspaces from the
    /// pool. Returns a handle the caller should abort on shutdown.
    ///
    /// `interval` controls how often the sweeper runs (e.g. 60 s).
    /// `idle_threshold` is the per-workspace age above which the entry is
    /// considered evictable (e.g. 300 s).
    pub fn spawn_idle_sweep(
        self: &Arc<Self>,
        watcher_pool: Arc<WatcherPool>,
        interval: Duration,
        idle_threshold: Duration,
    ) -> JoinHandle<()> {
        let pool = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            // First tick fires immediately; skip it so the sweeper waits one
            // interval before its first sweep.
            tick.tick().await;
            loop {
                tick.tick().await;
                let evicted = pool.sweep_idle_workspaces(&watcher_pool, idle_threshold).await;
                if !evicted.is_empty() {
                    info!(
                        count = evicted.len(),
                        "Idle sweep evicted {} workspace(s)",
                        evicted.len()
                    );
                }
            }
        })
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

    /// Shut down all workspaces in the pool, committing Tantivy writes and
    /// releasing file locks.
    ///
    /// Mirrors the per-session pattern at `handler.rs::teardown_loaded_workspace`.
    /// Drains the pool so no workspace is accessible after this returns.
    ///
    /// Per-workspace failures are logged and do not prevent the remaining
    /// workspaces from being shut down (infallible at the API level).
    ///
    /// Each workspace shutdown is bounded by `PER_WORKSPACE_SHUTDOWN_TIMEOUT`
    /// so a single hung Tantivy lock cannot stall daemon exit forever. A
    /// timed-out workspace is logged and skipped; its lock will be reclaimed
    /// by the OS when the process exits.
    pub async fn shutdown(&self) {
        const PER_WORKSPACE_SHUTDOWN_TIMEOUT: std::time::Duration =
            std::time::Duration::from_secs(2);

        // Take a write lock, drain the map, and release the lock before doing
        // any blocking work so we don't hold it across SearchIndex::shutdown().
        let entries: Vec<(String, WorkspaceEntry)> = {
            let mut guard = self.workspaces.write().await;
            guard.drain().collect()
        };

        for (workspace_id, entry) in entries {
            let Some(search_index) = entry.workspace.search_index.clone() else {
                continue;
            };
            let workspace_id_for_task = workspace_id.clone();
            let join = tokio::task::spawn_blocking(move || match search_index.lock() {
                Ok(idx) => idx.shutdown().map_err(|e| format!("shutdown error: {e}")),
                Err(poisoned) => {
                    let idx = poisoned.into_inner();
                    let _ = idx.shutdown();
                    Err(format!(
                        "recovered from poisoned mutex while shutting down {}",
                        workspace_id_for_task
                    ))
                }
            });

            match tokio::time::timeout(PER_WORKSPACE_SHUTDOWN_TIMEOUT, join).await {
                Ok(Ok(Ok(()))) => {
                    info!(
                        workspace_id = %workspace_id,
                        "Search index shut down, Tantivy file lock released"
                    );
                }
                Ok(Ok(Err(msg))) => {
                    warn!(
                        workspace_id = %workspace_id,
                        "Failed to shut down search index: {}",
                        msg
                    );
                }
                Ok(Err(join_err)) => {
                    warn!(
                        workspace_id = %workspace_id,
                        "Search index shutdown task panicked: {}",
                        join_err
                    );
                }
                Err(_) => {
                    warn!(
                        workspace_id = %workspace_id,
                        timeout_secs = PER_WORKSPACE_SHUTDOWN_TIMEOUT.as_secs(),
                        "Search index shutdown timed out; Tantivy lock may not release until process exit"
                    );
                }
            }
        }
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
        let julie_dir = if self.project_local_julie_dir {
            workspace_root.join(".julie")
        } else {
            self.indexes_dir.join(workspace_id).join("runtime")
        };
        std::fs::create_dir_all(&julie_dir).with_context(|| {
            format!(
                "Failed to create workspace runtime dir at {}",
                julie_dir.display()
            )
        })?;

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

/// Shut down a single removed workspace entry's search index. Mirrors the
/// per-workspace block used by `WorkspacePool::shutdown` so eviction releases
/// Tantivy file locks the same way as full daemon shutdown.
async fn shutdown_workspace_entry(workspace_id: &str, entry: WorkspaceEntry) {
    const PER_WORKSPACE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

    let Some(search_index) = entry.workspace.search_index.clone() else {
        return;
    };
    let workspace_id_for_task = workspace_id.to_string();
    let join = tokio::task::spawn_blocking(move || match search_index.lock() {
        Ok(idx) => idx.shutdown().map_err(|e| format!("shutdown error: {e}")),
        Err(poisoned) => {
            let idx = poisoned.into_inner();
            let _ = idx.shutdown();
            Err(format!(
                "recovered from poisoned mutex while shutting down {}",
                workspace_id_for_task
            ))
        }
    });

    match tokio::time::timeout(PER_WORKSPACE_SHUTDOWN_TIMEOUT, join).await {
        Ok(Ok(Ok(()))) => {
            info!(
                workspace_id = %workspace_id,
                "Search index shut down, Tantivy file lock released"
            );
        }
        Ok(Ok(Err(msg))) => {
            warn!(
                workspace_id = %workspace_id,
                "Failed to shut down search index: {}",
                msg
            );
        }
        Ok(Err(join_err)) => {
            warn!(
                workspace_id = %workspace_id,
                "Search index shutdown task panicked: {}",
                join_err
            );
        }
        Err(_) => {
            warn!(
                workspace_id = %workspace_id,
                timeout_secs = PER_WORKSPACE_SHUTDOWN_TIMEOUT.as_secs(),
                "Search index shutdown timed out; Tantivy lock may not release until process exit"
            );
        }
    }
}
