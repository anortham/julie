//! src/workspace_runtime/owner.rs
//! WorkspaceRuntime representation and owner startup.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use tokio::sync::{Mutex, watch};
use tokio_util::sync::CancellationToken;
use tracing::info;

use super::{RuntimeError, RuntimePhase};
use crate::handler::JulieServerHandler;
use crate::request_engine::types::WorkspaceBinding;

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

    /// Owner startup sequence: Start Watcher -> Owner
    pub async fn promote_to_owner(self: &Arc<Self>) -> Result<(), RuntimeError> {
        // Step 1: Check fault flag before starting watcher
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
}
