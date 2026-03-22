//! WatcherPool: reference-counted shared file watchers for the daemon.
//!
//! In daemon mode, multiple MCP sessions may connect to the same workspace.
//! Without sharing, each session would create its own `IncrementalIndexer`,
//! resulting in N duplicate OS-level file watches and N separate incremental
//! indexing pipelines for the same directory tree.
//!
//! `WatcherPool` solves this by maintaining one `IncrementalIndexer` per
//! workspace, reference-counted across sessions. When the last session
//! disconnects, a 5-minute grace period begins before the watcher is stopped.
//! If a new session connects within the grace period, the existing watcher
//! is reused with no gap in file watching.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::watcher::IncrementalIndexer;

/// Entry in the WatcherPool for a single workspace.
struct WatcherEntry {
    /// The active file watcher. `None` until `attach()` creates one.
    watcher: Option<IncrementalIndexer>,
    /// How many sessions are currently attached to this workspace.
    ref_count: usize,
    /// When this entry should be reaped (set when ref_count hits 0).
    grace_deadline: Option<Instant>,
}

/// Manages one `IncrementalIndexer` per workspace, shared across sessions.
///
/// Use `Arc<WatcherPool>` to share across tasks. The pool is `Send + Sync`
/// provided `IncrementalIndexer` is `Send`.
pub struct WatcherPool {
    entries: RwLock<HashMap<String, WatcherEntry>>,
    grace_period: Duration,
}

impl WatcherPool {
    /// Create a new empty pool.
    ///
    /// `grace_period` is how long after the last session disconnects before
    /// the watcher is stopped. Typical production value: 5 minutes.
    pub fn new(grace_period: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            grace_period,
        }
    }

    /// Increment the reference count for a workspace.
    ///
    /// Creates a new entry if none exists. Cancels any pending grace deadline.
    pub async fn increment_ref(&self, workspace_id: &str) {
        let mut guard = self.entries.write().await;
        let entry = guard
            .entry(workspace_id.to_string())
            .or_insert(WatcherEntry {
                watcher: None,
                ref_count: 0,
                grace_deadline: None,
            });
        entry.ref_count += 1;
        entry.grace_deadline = None; // cancel any pending grace period
        info!(
            workspace_id,
            ref_count = entry.ref_count,
            "Watcher ref_count incremented"
        );
    }

    /// Decrement the reference count for a workspace.
    ///
    /// If `ref_count` hits 0, a grace period deadline is set. The entry is
    /// not removed immediately — the reaper handles cleanup after the deadline.
    /// Clamped at 0: extra decrements are safe no-ops.
    pub async fn decrement_ref(&self, workspace_id: &str) {
        let mut guard = self.entries.write().await;
        if let Some(entry) = guard.get_mut(workspace_id) {
            entry.ref_count = entry.ref_count.saturating_sub(1);
            if entry.ref_count == 0 {
                entry.grace_deadline = Some(Instant::now() + self.grace_period);
                info!(workspace_id, "Watcher grace period started");
            } else {
                info!(
                    workspace_id,
                    ref_count = entry.ref_count,
                    "Watcher ref_count decremented"
                );
            }
        }
        // If the entry doesn't exist, the decrement is a no-op.
    }

    /// Returns the current reference count for a workspace (0 if not tracked).
    pub async fn ref_count(&self, workspace_id: &str) -> usize {
        let guard = self.entries.read().await;
        guard
            .get(workspace_id)
            .map(|e| e.ref_count)
            .unwrap_or(0)
    }

    /// Returns whether a grace deadline is currently set for this workspace.
    pub async fn has_grace_deadline(&self, workspace_id: &str) -> bool {
        let guard = self.entries.read().await;
        guard
            .get(workspace_id)
            .and_then(|e| e.grace_deadline)
            .is_some()
    }

    /// Attach a session to a workspace's watcher.
    ///
    /// Increments the ref count. If no watcher exists for this workspace,
    /// creates and starts an `IncrementalIndexer`. Cancels any pending grace
    /// deadline (reuse-within-grace path).
    pub async fn attach(
        &self,
        workspace_id: &str,
        workspace: &crate::workspace::JulieWorkspace,
    ) -> anyhow::Result<()> {
        let mut guard = self.entries.write().await;
        let entry = guard
            .entry(workspace_id.to_string())
            .or_insert(WatcherEntry {
                watcher: None,
                ref_count: 0,
                grace_deadline: None,
            });
        entry.ref_count += 1;
        entry.grace_deadline = None;

        // Create watcher on first attach (or if it was reaped and recreated).
        if entry.watcher.is_none() {
            if let (Some(db), Some(search_index)) = (&workspace.db, &workspace.search_index) {
                let extractor_mgr = Arc::new(crate::extractors::ExtractorManager::new());
                let mut indexer = IncrementalIndexer::new(
                    workspace.root.clone(),
                    db.clone(),
                    extractor_mgr,
                    Some(search_index.clone()),
                    workspace.embedding_provider.clone(),
                )?;
                indexer.start_watching().await?;
                entry.watcher = Some(indexer);
                info!(workspace_id, "File watcher created and started");
            }
        }
        Ok(())
    }

    /// Detach a session. Starts the grace period when ref_count hits 0.
    pub async fn detach(&self, workspace_id: &str) {
        self.decrement_ref(workspace_id).await;
    }

    /// Reap all entries whose grace deadline has passed.
    ///
    /// Stops the `IncrementalIndexer` for each reaped entry (if one exists)
    /// by spawning a background task. Returns the list of reaped workspace IDs.
    ///
    /// Call this from a periodic background task (see `spawn_reaper`).
    pub async fn reap_expired(&self) -> Vec<String> {
        let mut guard = self.entries.write().await;
        let now = Instant::now();
        let mut reaped = Vec::new();

        // Collect entries to remove (retain returns false to remove).
        let mut to_stop: Vec<(String, IncrementalIndexer)> = Vec::new();

        guard.retain(|id, entry| {
            if let Some(deadline) = entry.grace_deadline {
                if now >= deadline {
                    // Extract watcher before removing the entry.
                    if let Some(watcher) = entry.watcher.take() {
                        to_stop.push((id.clone(), watcher));
                    }
                    info!(workspace_id = %id, "Reaping expired watcher");
                    reaped.push(id.clone());
                    return false; // remove from map
                }
            }
            true // keep
        });

        // Release the write lock before spawning stop tasks.
        drop(guard);

        for (id, mut watcher) in to_stop {
            tokio::spawn(async move {
                if let Err(e) = watcher.stop().await {
                    warn!(workspace_id = %id, "Failed to stop watcher during reap: {}", e);
                }
            });
        }

        reaped
    }

    /// Spawn a background task that calls `reap_expired` every `interval`.
    ///
    /// Returns the `JoinHandle` — the caller should hold it (or abort it) for
    /// graceful shutdown. The task loops forever until aborted.
    pub fn spawn_reaper(self: &Arc<Self>, interval: Duration) -> tokio::task::JoinHandle<()> {
        let pool = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                let reaped = pool.reap_expired().await;
                if !reaped.is_empty() {
                    info!(count = reaped.len(), "Reaped expired watchers");
                }
            }
        })
    }
}
