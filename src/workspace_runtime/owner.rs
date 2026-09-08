//! src/workspace_runtime/owner.rs
//! WorkspaceRuntime representation, dynamic follower probe loop, and owner recovery.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use super::{RuntimeError, RuntimePhase};
use crate::handler::JulieServerHandler;
use crate::request_engine::types::WorkspaceBinding;
use julie_core::workspace::leader_lock::{AcquireError, DaemonLockGuard};
use julie_core::workspace::ownership::OwnerEpoch;

pub struct OwnerState {
    pub epoch: Arc<OwnerEpoch>,
}

/// Independent runtime representation for an active workspace.
pub struct WorkspaceRuntime {
    pub binding: WorkspaceBinding,
    pub phase: watch::Receiver<RuntimePhase>,
    pub(crate) phase_tx: watch::Sender<RuntimePhase>,
    pub handler: Arc<JulieServerHandler>,
    pub active_requests: AtomicUsize,
    pub in_flight_commits: AtomicUsize,
    pub current_epoch: AtomicU64,
    pub(crate) owner_state: Mutex<Option<OwnerState>>,
    pub(crate) probe_cancel: std::sync::Mutex<Option<CancellationToken>>,
    pub(crate) shutdown_token: CancellationToken,
    pub(crate) last_activity: AtomicI64,
    pub(crate) fault_flag: Mutex<Option<String>>,
}

impl WorkspaceRuntime {
    pub fn new(
        binding: WorkspaceBinding,
        initial_phase: RuntimePhase,
        handler: Arc<JulieServerHandler>,
    ) -> Arc<Self> {
        let (phase_tx, phase) = watch::channel(initial_phase);
        Self::with_phase_channel(binding, phase_tx, phase, handler)
    }

    pub fn with_phase_channel(
        binding: WorkspaceBinding,
        phase_tx: watch::Sender<RuntimePhase>,
        phase: watch::Receiver<RuntimePhase>,
        handler: Arc<JulieServerHandler>,
    ) -> Arc<Self> {
        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        Arc::new(Self {
            binding,
            phase,
            phase_tx,
            handler,
            active_requests: AtomicUsize::new(0),
            in_flight_commits: AtomicUsize::new(0),
            current_epoch: AtomicU64::new(0),
            owner_state: Mutex::new(None),
            probe_cancel: std::sync::Mutex::new(None),
            shutdown_token: CancellationToken::new(),
            last_activity: AtomicI64::new(now_millis),
            fault_flag: Mutex::new(None),
        })
    }

    pub fn touch(&self) {
        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        self.last_activity.store(now_millis, Ordering::Relaxed);
    }

    pub fn last_activity(&self) -> i64 {
        self.last_activity.load(Ordering::Relaxed)
    }

    pub fn leader_lock_path(&self) -> PathBuf {
        self.binding.index_root.join("leader.lock")
    }

    pub async fn inject_fault(&self, fault: &str) {
        let mut guard = self.fault_flag.lock().await;
        *guard = Some(fault.to_string());
    }

    /// Transition to a new phase after state machine validation.
    pub fn set_phase(&self, next: RuntimePhase) -> Result<(), RuntimeError> {
        let current = self.phase.borrow().clone();
        current.validate_transition(&next)?;
        self.phase_tx.send_replace(next);
        Ok(())
    }

    /// Spawns the background dynamic follower probe loop (500ms + 0–100ms jitter).
    pub fn start_follower_probe_loop(self: &Arc<Self>, probe_interval: Duration) {
        let cancel = CancellationToken::new();
        {
            let mut guard = self.probe_cancel.lock().unwrap();
            *guard = Some(cancel.clone());
        }

        let runtime = Arc::clone(self);
        tokio::spawn(async move {
            info!(
                workspace_id = %runtime.binding.workspace_id,
                "Started follower dynamic leader election probe loop"
            );

            loop {
                if cancel.is_cancelled() || runtime.shutdown_token.is_cancelled() {
                    break;
                }

                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                let jitter = (nanos % 101) as u64;
                let sleep_dur = probe_interval + Duration::from_millis(jitter);
                tokio::select! {
                    _ = tokio::time::sleep(sleep_dur) => {}
                    _ = cancel.cancelled() => break,
                    _ = runtime.shutdown_token.cancelled() => break,
                }

                if !runtime.phase.borrow().is_follower() {
                    break;
                }

                match DaemonLockGuard::try_acquire(&runtime.leader_lock_path()) {
                    Ok(guard) => {
                        info!(
                            workspace_id = %runtime.binding.workspace_id,
                            "Follower acquired workspace leader lock — promoting to Owner"
                        );
                        if let Err(e) = runtime.promote_to_owner(guard).await {
                            error!(
                                workspace_id = %runtime.binding.workspace_id,
                                error = %e,
                                "Failed owner promotion sequence"
                            );
                            let _ = runtime.set_phase(RuntimePhase::Failed {
                                code: format!("PROMOTION_FAILED: {e}"),
                            });
                        }
                        break;
                    }
                    Err(AcquireError::AlreadyHeld(_)) => {
                        continue;
                    }
                    Err(AcquireError::Io { path, source }) => {
                        error!(
                            workspace_id = %runtime.binding.workspace_id,
                            path = %path.display(),
                            error = %source,
                            "Fatal I/O error during follower leader probe"
                        );
                        let _ = runtime.set_phase(RuntimePhase::Failed {
                            code: format!("LOCK_IO_ERROR: {source}"),
                        });
                        break;
                    }
                }
            }
        });
    }

    /// Complete owner promotion sequence:
    /// Follower -> Recovering { epoch } -> Reconcile -> Start Watcher -> Owner { epoch }
    pub async fn promote_to_owner(
        self: &Arc<Self>,
        guard: DaemonLockGuard,
    ) -> Result<(), RuntimeError> {
        let epoch_num = self.current_epoch.fetch_add(1, Ordering::SeqCst) + 1;

        // Step 1: Transition to Recovering
        self.set_phase(RuntimePhase::Recovering { epoch: epoch_num })?;
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;

        let owner_epoch = Arc::new(OwnerEpoch::new(
            epoch_num,
            self.binding.workspace_id.clone(),
            guard,
        ));

        // Step 2: Reconcile canonical SQLite vs Tantivy projection lag
        if let Err(err) = self.reconcile_projection_lag(&owner_epoch).await {
            warn!(
                workspace_id = %self.binding.workspace_id,
                error = %err,
                "Projection lag reconciliation failed during owner recovery"
            );
        }

        // Step 3: Check fault flag before starting watcher
        {
            let fault_guard = self.fault_flag.lock().await;
            if let Some(ref fault) = *fault_guard {
                if fault == "FAIL_WATCHER_STARTUP" {
                    let _ = self.set_phase(RuntimePhase::Failed {
                        code: "INJECTED_WATCHER_STARTUP_FAILURE".into(),
                    });
                    return Err(RuntimeError::Failed(
                        "Injected watcher startup failure".into(),
                    ));
                }
            }
        }

        // Step 4: Start single file watcher
        {
            let mut ws_guard = self.handler.workspace.write().await;
            if let Some(ref mut ws) = *ws_guard {
                if ws.watcher.is_none() && ws.config.incremental_updates {
                    if let Err(e) = ws.initialize_file_watcher() {
                        let _ = self.set_phase(RuntimePhase::Failed {
                            code: format!("WATCHER_INIT_FAILED: {e}"),
                        });
                        return Err(RuntimeError::Internal(format!(
                            "Failed to initialize watcher: {e}"
                        )));
                    }
                }
                if let Err(e) = ws
                    .start_file_watching_with_epoch(true, Some(Arc::clone(&owner_epoch)))
                    .await
                {
                    let _ = self.set_phase(RuntimePhase::Failed {
                        code: format!("WATCHER_START_FAILED: {e}"),
                    });
                    return Err(RuntimeError::Internal(format!(
                        "Failed to start watcher: {e}"
                    )));
                }
            }
        }

        // Store active owner state
        {
            let mut owner_guard = self.owner_state.lock().await;
            *owner_guard = Some(OwnerState {
                epoch: Arc::clone(&owner_epoch),
            });
        }
        {
            let mut handler_epoch = self.handler.leadership.owner_epoch.write().await;
            *handler_epoch = Some(Arc::clone(&owner_epoch));
        }

        // Step 5: Publish Owner
        self.set_phase(RuntimePhase::Owner { epoch: epoch_num })?;
        info!(
            workspace_id = %self.binding.workspace_id,
            epoch = epoch_num,
            "Successfully promoted to Owner"
        );

        Ok(())
    }

    async fn reconcile_projection_lag(&self, owner_epoch: &OwnerEpoch) -> Result<(), RuntimeError> {
        let snapshot = match self.handler.primary_workspace_snapshot().await {
            Ok(s) => s,
            Err(_) => return Ok(()),
        };

        let search_index = snapshot.search_index;
        let workspace_id = snapshot.binding.workspace_id.clone();
        let db_arc = snapshot.database;

        let web_edges_rebuilt = {
            let mut db = db_arc.lock().unwrap_or_else(|p| p.into_inner());
            julie_pipeline::indexing_core::web_edges::ensure_web_edges_current(
                &mut db,
                &workspace_id,
            )
            .map_err(|e| RuntimeError::Internal(format!("Web-edge reconciliation failed: {e}")))?
        };
        if web_edges_rebuilt {
            info!(%workspace_id, "Web-edge projection reconciled from canonical SQLite state");
        }

        let search_index = match search_index {
            Some(index) => index,
            None => {
                let tantivy_path = self
                    .handler
                    .workspace_tantivy_dir_for(&workspace_id)
                    .await
                    .map_err(|e| {
                        RuntimeError::Internal(format!("Failed to resolve Tantivy dir: {e}"))
                    })?;
                std::fs::create_dir_all(&tantivy_path).map_err(|e| {
                    RuntimeError::Internal(format!("Failed to create Tantivy dir: {e}"))
                })?;
                let configs = crate::search::LanguageConfigs::load_embedded();
                let index = tokio::task::spawn_blocking(move || {
                    julie_index::search::SearchIndex::open_or_create_with_language_configs(
                        &tantivy_path,
                        &configs,
                    )
                })
                .await
                .map_err(|e| RuntimeError::Internal(format!("Tantivy open join error: {e}")))?
                .map_err(|e| {
                    RuntimeError::Internal(format!("Failed to open/create Tantivy index: {e}"))
                })?;
                Arc::new(index)
            }
        };

        let permit = owner_epoch
            .acquire_writer(&self.handler.mutation_gate_registry)
            .await
            .map_err(|e| RuntimeError::Internal(format!("Failed to acquire writer permit: {e}")))?;

        let coordinator = super::recovery::ProjectionRecoveryCoordinator::new(workspace_id);
        coordinator
            .reconcile_if_needed(&db_arc, &search_index, &permit)
            .await
            .map_err(|e| {
                RuntimeError::Internal(format!("Projection reconciliation failed: {e}"))
            })?;
        Ok(())
    }
}
