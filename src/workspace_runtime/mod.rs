//! src/workspace_runtime/mod.rs
//! Workspace runtime lifecycle management, phases, leases, and errors.

pub mod builder;
pub mod continuation;
pub mod continuation_store;
pub mod dirty_queue;
pub mod edit_journal;
pub mod manager;
pub mod owner;
pub mod publication;
pub mod recovery;
pub mod scheduler;
pub mod shutdown;
pub mod source_edit;
pub mod source_edit_ops;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use thiserror::Error;

pub use builder::WorkspaceRuntimeManagerBuilder;
pub use continuation::{
    ContinuationBinding, ContinuationError, ContinuationFailure, DEFAULT_CONTINUATION_TTL_SECS,
    MAX_SNAPSHOT_SIZE_BYTES, MAX_WORKSPACE_CONTINUATION_BUDGET_BYTES, is_valid_continuation_token,
    validate_handle_binding,
};
pub use continuation_store::ContinuationStore;
pub use dirty_queue::{
    DEFAULT_DIRTY_QUEUE_CAPACITY, DirtyChunk, DirtyEntry, DirtyOp, DirtyQueue,
    normalize_relative_path,
};
pub use edit_journal::{
    EditDisposition, EditJournal, JournalFileEntry, JournalFileState, JournalState, RecoveryAction,
};
pub use julie_core::workspace::ownership::{RuntimePhaseKind, allowed_transition};
pub use manager::{RuntimeKey, WorkspaceRuntimeManager};
pub use owner::WorkspaceRuntime;
pub use publication::{SnapshotError, WorkspaceReadSnapshot};
pub use recovery::ProjectionRecoveryCoordinator;
pub use scheduler::{
    DEFAULT_ADMISSION_TIMEOUT, DEFAULT_MAX_SOURCE_SIZE_BYTES, DEFAULT_SCHEDULING_QUANTUM,
    FileCommitter, ProcessFairScheduler, QuantumReport, SchedulerConfig, SchedulerError,
    WorkspaceScheduler,
};
pub use source_edit::{
    DEFAULT_MAX_EDIT_SOURCE_BYTES, ENV_MAX_EDIT_SOURCE_BYTES, MAX_MAX_EDIT_SOURCE_BYTES,
    MIN_MAX_EDIT_SOURCE_BYTES, PreparedSourceChange, SourceEditConfig, SourceEditCoordinator,
    SourceEditError, acquire_source_edit_lock, read_bounded_source,
};

/// Lifecycle phases of a workspace runtime: the runtime is the writer until it fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimePhase {
    /// Active index owner: exclusive writer for SQLite and Tantivy.
    Owner { epoch: u64 },
    /// Unrecoverable error state.
    Failed { code: String },
}

#[derive(Debug, Error)]
pub enum TransitionError {
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: RuntimePhase,
        to: RuntimePhase,
    },
    #[error("Failed cannot become Owner; runtime is in failed state")]
    FailedToOwnerForbidden,
    #[error("Owner epoch cannot decrement: current {current}, next {next}")]
    EpochDecrementForbidden { current: u64, next: u64 },
}

impl RuntimePhase {
    /// Pure discriminant kind for state machine transition evaluation.
    pub fn kind(&self) -> RuntimePhaseKind {
        match self {
            Self::Owner { .. } => RuntimePhaseKind::Owner,
            Self::Failed { .. } => RuntimePhaseKind::Failed,
        }
    }

    /// Validate state machine transitions.
    pub fn validate_transition(&self, next: &RuntimePhase) -> Result<(), TransitionError> {
        let from_kind = self.kind();
        let to_kind = next.kind();

        if !allowed_transition(from_kind, to_kind) {
            return match (from_kind, to_kind) {
                (RuntimePhaseKind::Failed, RuntimePhaseKind::Owner) => {
                    Err(TransitionError::FailedToOwnerForbidden)
                }
                _ => Err(TransitionError::InvalidTransition {
                    from: self.clone(),
                    to: next.clone(),
                }),
            };
        }

        if let (RuntimePhase::Owner { epoch: cur }, RuntimePhase::Owner { epoch: next }) =
            (self, next)
            && next <= cur
        {
            return Err(TransitionError::EpochDecrementForbidden {
                current: *cur,
                next: *next,
            });
        }

        Ok(())
    }

    pub fn can_transition_to(&self, next: &RuntimePhase) -> bool {
        self.validate_transition(next).is_ok()
    }

    pub fn is_owner(&self) -> bool {
        matches!(self, RuntimePhase::Owner { .. })
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, RuntimePhase::Failed { .. })
    }
}

/// RAII lease granted to incoming requests.
/// Decrements request use count on Drop without affecting runtime lifetime.
pub struct RuntimeLease {
    pub(crate) runtime: Arc<WorkspaceRuntime>,
}

impl RuntimeLease {
    pub fn runtime(&self) -> &Arc<WorkspaceRuntime> {
        &self.runtime
    }

    pub fn phase(&self) -> RuntimePhase {
        self.runtime.phase.borrow().clone()
    }
}

impl std::ops::Deref for RuntimeLease {
    type Target = WorkspaceRuntime;
    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

impl Drop for RuntimeLease {
    fn drop(&mut self) {
        self.runtime.active_requests.fetch_sub(1, Ordering::SeqCst);
        self.runtime.touch();
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Failed to acquire leader lock: {0}")]
    LockAcquire(String),
    #[error("Runtime failed: {0}")]
    Failed(String),
    #[error("Transition error: {0}")]
    Transition(#[from] TransitionError),
    #[error("Timeout waiting for runtime readiness")]
    Timeout,
    #[error("Cancelled by caller")]
    Cancelled,
    #[error("Internal error: {0}")]
    Internal(String),
}
