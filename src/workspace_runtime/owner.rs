//! src/workspace_runtime/owner.rs
//! WorkspaceRuntime representation and owner startup.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::{RuntimeError, RuntimePhase};
use crate::handler::JulieServerHandler;
use crate::request_engine::types::WorkspaceBinding;
use julie_core::workspace::mutation_gate::acquire_gate;

/// Independent runtime representation for an active workspace.
pub struct WorkspaceRuntime {
    pub binding: WorkspaceBinding,
    pub phase: watch::Receiver<RuntimePhase>,
    pub(crate) phase_tx: watch::Sender<RuntimePhase>,
    pub handler: Arc<JulieServerHandler>,
    pub active_requests: AtomicUsize,
    pub in_flight_commits: AtomicUsize,
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

    /// Owner startup sequence: Reconcile -> Start Watcher -> Owner
    pub async fn promote_to_owner(self: &Arc<Self>) -> Result<(), RuntimeError> {
        // Step 1: Reconcile canonical SQLite vs Tantivy projection lag
        if let Err(err) = self.reconcile_projection_lag().await {
            warn!(
                workspace_id = %self.binding.workspace_id,
                error = %err,
                "Projection lag reconciliation failed during owner recovery"
            );
        }

        // Step 2: Check fault flag before starting watcher
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

        // Step 3: Start single file watcher
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
                if let Err(e) = ws.start_file_watching(true).await {
                    let _ = self.set_phase(RuntimePhase::Failed {
                        code: format!("WATCHER_START_FAILED: {e}"),
                    });
                    return Err(RuntimeError::Internal(format!(
                        "Failed to start watcher: {e}"
                    )));
                }
            }
        }

        // Step 4: Publish Owner
        self.set_phase(RuntimePhase::Owner)?;
        info!(
            workspace_id = %self.binding.workspace_id,
            "Successfully promoted to Owner"
        );

        Ok(())
    }

    async fn reconcile_projection_lag(&self) -> Result<(), RuntimeError> {
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

        let guard = acquire_gate(&workspace_id).await;
        let coordinator = super::recovery::ProjectionRecoveryCoordinator::new(workspace_id);
        coordinator
            .reconcile_if_needed(&db_arc, &search_index, &guard)
            .await
            .map_err(|e| {
                RuntimeError::Internal(format!("Projection reconciliation failed: {e}"))
            })?;
        Ok(())
    }
}
