//! src/workspace_runtime/manager.rs
//! Manager providing single-flight construction, caching, and bounded idle eviction.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::{RuntimeError, RuntimeLease, RuntimePhase, WorkspaceRuntime};
use crate::handler::JulieServerHandler;
use crate::paths::RegistryPaths;
use crate::registry::database::DaemonDatabase;
use crate::request_engine::types::WorkspaceBinding;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};
use julie_core::workspace::leader_lock::{AcquireError, DaemonLockGuard};

pub const DEFAULT_MAX_IDLE_RUNTIMES: usize = 8;
pub const DEFAULT_IDLE_EXPIRY_DURATION: Duration = Duration::from_secs(60);
pub const DEFAULT_PROBE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeKey {
    pub workspace_id: String,
    pub index_root: PathBuf,
}

#[derive(Clone, Default)]
pub struct ManagerTestBarriers {
    pub commit_started: Arc<tokio::sync::Notify>,
    pub commit_barrier: Arc<tokio::sync::Notify>,
}

pub(crate) enum SlotState {
    Ready(Arc<WorkspaceRuntime>),
    Initializing(tokio::sync::broadcast::Sender<Result<Arc<WorkspaceRuntime>, String>>),
}

struct InitGuard {
    slots: Arc<RwLock<HashMap<RuntimeKey, SlotState>>>,
    key: RuntimeKey,
    active: bool,
}

impl Drop for InitGuard {
    fn drop(&mut self) {
        if self.active {
            let slots = Arc::clone(&self.slots);
            let key = self.key.clone();
            tokio::spawn(async move {
                let mut guard = slots.write().await;
                if let Some(SlotState::Initializing(_)) = guard.get(&key) {
                    guard.remove(&key);
                }
            });
        }
    }
}

pub struct WorkspaceRuntimeManager {
    pub(crate) registry_paths: RegistryPaths,
    pub(crate) slots: Arc<RwLock<HashMap<RuntimeKey, SlotState>>>,
    pub(crate) template_handler: Option<Arc<JulieServerHandler>>,
    pub(crate) probe_interval: Duration,
    pub(crate) idle_timeout: Duration,
    pub(crate) max_idle_runtimes: usize,
    pub(crate) test_barriers: Option<ManagerTestBarriers>,
    pub(crate) fault_flag: Option<String>,
    pub(crate) eviction_task: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

pub use super::builder::WorkspaceRuntimeManagerBuilder;

impl WorkspaceRuntimeManager {
    pub fn new(registry_paths: RegistryPaths) -> Arc<Self> {
        Self::builder(registry_paths).build()
    }

    pub fn builder(registry_paths: RegistryPaths) -> WorkspaceRuntimeManagerBuilder {
        WorkspaceRuntimeManagerBuilder::new(registry_paths)
    }

    pub async fn acquire(
        &self,
        binding: &WorkspaceBinding,
        deadline: Option<Instant>,
        cancellation: &CancellationToken,
    ) -> Result<RuntimeLease, RuntimeError> {
        if cancellation.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }

        let key = RuntimeKey {
            workspace_id: binding.workspace_id.clone(),
            index_root: binding.index_root.clone(),
        };

        let runtime = loop {
            if cancellation.is_cancelled() {
                return Err(RuntimeError::Cancelled);
            }
            if deadline.is_some_and(|dl| Instant::now() >= dl) {
                return Err(RuntimeError::Timeout);
            }

            // Fast-path read check
            {
                let read_guard = self.slots.read().await;
                if let Some(slot) = read_guard.get(&key) {
                    match slot {
                        SlotState::Ready(rt) => {
                            if !rt.phase.borrow().is_draining() && !rt.phase.borrow().is_terminal()
                            {
                                break Arc::clone(rt);
                            }
                        }
                        SlotState::Initializing(tx) => {
                            let mut rx = tx.subscribe();
                            drop(read_guard);
                            match rx.recv().await {
                                Ok(Ok(rt)) => break rt,
                                Ok(Err(e)) => return Err(RuntimeError::Internal(e)),
                                Err(_) => continue,
                            }
                        }
                    }
                }
            }

            // Write-path insertion
            let mut write_guard = self.slots.write().await;
            if let Some(slot) = write_guard.get(&key) {
                match slot {
                    SlotState::Ready(rt) => {
                        if !rt.phase.borrow().is_draining() && !rt.phase.borrow().is_terminal() {
                            break Arc::clone(rt);
                        } else if rt.phase.borrow().is_terminal() {
                            write_guard.remove(&key);
                        }
                    }
                    SlotState::Initializing(tx) => {
                        let mut rx = tx.subscribe();
                        drop(write_guard);
                        match rx.recv().await {
                            Ok(Ok(rt)) => break rt,
                            Ok(Err(e)) => return Err(RuntimeError::Internal(e)),
                            Err(_) => continue,
                        }
                    }
                }
            }

            let (tx, _) = tokio::sync::broadcast::channel(1);
            write_guard.insert(key.clone(), SlotState::Initializing(tx.clone()));
            drop(write_guard);

            let mut init_guard = InitGuard {
                slots: Arc::clone(&self.slots),
                key: key.clone(),
                active: true,
            };

            let construct_res = self.construct_runtime(binding).await;
            init_guard.active = false;

            match construct_res {
                Ok(rt) => {
                    self.slots
                        .write()
                        .await
                        .insert(key.clone(), SlotState::Ready(Arc::clone(&rt)));
                    let _ = tx.send(Ok(Arc::clone(&rt)));
                    break rt;
                }
                Err((Some(rt), err)) => {
                    self.slots
                        .write()
                        .await
                        .insert(key.clone(), SlotState::Ready(Arc::clone(&rt)));
                    let _ = tx.send(Err(err.to_string()));
                    return Err(err);
                }
                Err((None, err)) => {
                    self.slots.write().await.remove(&key);
                    let _ = tx.send(Err(err.to_string()));
                    return Err(err);
                }
            }
        };

        runtime
            .active_requests
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        runtime.touch();

        Ok(RuntimeLease { runtime })
    }

    async fn construct_runtime(
        &self,
        binding: &WorkspaceBinding,
    ) -> Result<Arc<WorkspaceRuntime>, (Option<Arc<WorkspaceRuntime>>, RuntimeError)> {
        std::fs::create_dir_all(&binding.index_root).map_err(|e| {
            (
                None,
                RuntimeError::Internal(format!("Failed to create index directory: {e}")),
            )
        })?;

        let lock_path = binding.index_root.join("leader.lock");
        let (initial_phase, guard_opt) = match DaemonLockGuard::try_acquire(&lock_path) {
            Ok(guard) => (RuntimePhase::Opening, Some(guard)),
            Err(AcquireError::AlreadyHeld(_)) => (RuntimePhase::Follower, None),
            Err(AcquireError::Io { path, source }) => {
                return Err((
                    None,
                    RuntimeError::LockAcquire(format!(
                        "Lock IO error at {}: {}",
                        path.display(),
                        source
                    )),
                ));
            }
        };

        let startup_hint = WorkspaceStartupHint {
            path: binding.root.clone(),
            source: Some(WorkspaceStartupSource::Cli),
        };

        let daemon_db = self
            .template_handler
            .as_ref()
            .and_then(|th| th.daemon_db.clone())
            .or_else(|| {
                DaemonDatabase::open(&self.registry_paths.registry_db())
                    .ok()
                    .map(Arc::new)
            });

        let (phase_tx, phase_rx) = tokio::sync::watch::channel(initial_phase);
        let leadership = crate::leadership::LeadershipState::dynamic(phase_rx.clone());

        let mut handler = JulieServerHandler::new_in_process_with_daemon_db(
            startup_hint,
            None,
            leadership,
            Some(binding.index_root.clone()),
            daemon_db,
        )
        .await
        .map_err(|e| {
            (
                None,
                RuntimeError::Internal(format!("Failed to build handler: {e}")),
            )
        })?;

        if let Some(ref th) = self.template_handler {
            handler.session_metrics = Arc::clone(&th.session_metrics);
        }

        handler
            .initialize_workspace_with_force(None, false)
            .await
            .map_err(|e| {
                (
                    None,
                    RuntimeError::Internal(format!("Failed to initialize workspace: {e}")),
                )
            })?;

        let runtime = WorkspaceRuntime::with_phase_channel(
            binding.clone(),
            phase_tx,
            phase_rx,
            Arc::new(handler),
        );

        if let Some(ref fault) = self.fault_flag {
            runtime.inject_fault(fault).await;
        }

        match guard_opt {
            Some(guard) => {
                if let Err(e) = runtime.promote_to_owner(guard).await {
                    return Err((Some(runtime), e));
                }
            }
            None => {
                runtime.start_follower_probe_loop(self.probe_interval);
            }
        }

        Ok(runtime)
    }

    pub fn is_idle(&self, binding: &WorkspaceBinding) -> bool {
        let key = RuntimeKey {
            workspace_id: binding.workspace_id.clone(),
            index_root: binding.index_root.clone(),
        };
        self.slots.try_read().map_or(false, |guard| {
            guard.get(&key).map_or(true, |slot| match slot {
                SlotState::Ready(rt) => {
                    rt.active_requests.load(std::sync::atomic::Ordering::SeqCst) == 0
                        && rt
                            .in_flight_commits
                            .load(std::sync::atomic::Ordering::SeqCst)
                            == 0
                }
                _ => true,
            })
        })
    }

    pub async fn contains(&self, binding: &WorkspaceBinding) -> bool {
        let key = RuntimeKey {
            workspace_id: binding.workspace_id.clone(),
            index_root: binding.index_root.clone(),
        };
        self.slots
            .read()
            .await
            .get(&key)
            .map_or(false, |slot| match slot {
                SlotState::Ready(rt) => {
                    !rt.phase.borrow().is_draining() && !rt.phase.borrow().is_terminal()
                }
                _ => false,
            })
    }

    pub fn watch_phase(
        &self,
        binding: &WorkspaceBinding,
    ) -> Option<tokio::sync::watch::Receiver<RuntimePhase>> {
        let key = RuntimeKey {
            workspace_id: binding.workspace_id.clone(),
            index_root: binding.index_root.clone(),
        };
        self.slots
            .try_read()
            .ok()
            .and_then(|guard| match guard.get(&key)? {
                SlotState::Ready(rt) => Some(rt.phase.clone()),
                _ => None,
            })
    }

    pub async fn trigger_test_commit(&self, binding: &WorkspaceBinding) {
        let key = RuntimeKey {
            workspace_id: binding.workspace_id.clone(),
            index_root: binding.index_root.clone(),
        };
        let rt = {
            let guard = self.slots.read().await;
            match guard.get(&key) {
                Some(SlotState::Ready(r)) => Arc::clone(r),
                _ => return,
            }
        };

        rt.in_flight_commits
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let barriers = self.test_barriers.clone();

        tokio::spawn(async move {
            if let Some(ref b) = barriers {
                b.commit_started.notify_waiters();
                b.commit_barrier.notified().await;
            }

            let ws_guard = rt.handler.workspace.read().await;
            if let Some(ref ws) = *ws_guard {
                if let Some(ref db_arc) = ws.db {
                    let mut db = match db_arc.lock() {
                        Ok(db) => db,
                        Err(p) => p.into_inner(),
                    };
                    if let Err(e) = db.conn.execute(
                        "INSERT INTO canonical_revisions (workspace_id, kind, cleaned_file_count, file_count, symbol_count, relationship_count, identifier_count, type_count, created_at) VALUES (?1, 'incremental', 0, 1, 1, 1, 1, 1, 1000)",
                        rusqlite::params![&rt.binding.workspace_id],
                    ) {
                        tracing::error!("Failed to insert canonical revision: {:?}", e);
                    }
                    let _ = db.checkpoint_wal();
                }
            }

            rt.in_flight_commits
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        });
    }

    pub async fn evict_idle_runtimes(&self) {
        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let expiry_millis = self.idle_timeout.as_millis() as i64;

        let mut to_evict = Vec::new();
        {
            let slots = self.slots.read().await;
            let mut idle_candidates = Vec::new();

            for (key, slot) in slots.iter() {
                if let SlotState::Ready(rt) = slot {
                    if rt.active_requests.load(std::sync::atomic::Ordering::SeqCst) == 0
                        && rt
                            .in_flight_commits
                            .load(std::sync::atomic::Ordering::SeqCst)
                            == 0
                    {
                        let idle_for = now_millis - rt.last_activity();
                        idle_candidates.push((key.clone(), idle_for, Arc::clone(rt)));
                    }
                }
            }

            idle_candidates.sort_by_key(|(_, idle_for, _)| -idle_for);

            for (idx, (key, idle_for, rt)) in idle_candidates.into_iter().enumerate() {
                if idle_for >= expiry_millis || idx >= self.max_idle_runtimes {
                    to_evict.push((key, rt));
                }
            }
        }

        for (key, rt) in to_evict {
            info!(workspace_id = %key.workspace_id, "Evicting idle workspace runtime");
            self.slots.write().await.remove(&key);

            tokio::spawn(async move {
                crate::workspace_runtime::shutdown::drain_and_shutdown(&rt, Duration::from_secs(5))
                    .await;
            });
        }
    }

    pub(crate) fn start_eviction_loop(self: &Arc<Self>) {
        let manager = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            while let Some(mgr) = {
                interval.tick().await;
                manager.upgrade()
            } {
                mgr.evict_idle_runtimes().await;
            }
        });
        *self.eviction_task.lock().unwrap() = Some(task);
    }
}
