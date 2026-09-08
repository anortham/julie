//! crates/julie-core/src/workspace/ownership.rs
//! Ownership proof tokens, publication stamps, and writer permits for fenced index mutations.

use crate::workspace::leader_lock::DaemonLockGuard;
use crate::workspace::mutation_gate::{MutationGuard, Registry as MutationGateRegistry};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Notify;

/// Point-in-time stamp identifying an atomic revision publication across stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationStamp {
    pub epoch: u64,
    pub canonical_revision: u64,
    pub projected_revision: u64,
    pub generation: u64,
}

/// Metadata payload embedded into Tantivy commit via `PreparedCommit::set_payload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TantivyCommitPayload {
    pub generation: u64,
    pub revision: u64,
    pub epoch: u64,
}

/// Verification state of disk files prior to canonical commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceCheckState {
    Verified { files_checked: usize },
    RecheckRequeued { requeued_count: usize },
    HashMismatch,
    Unchecked,
}

/// Predicate evaluating whether projection recovery is needed between
/// SQLite canonical truth and the derived Tantivy projection.
pub fn needs_projection_recovery(canonical: u64, projected: Option<u64>) -> bool {
    match projected {
        Some(revision) => revision != canonical,
        None => true,
    }
}

#[derive(Debug)]
pub enum OwnershipError {
    Draining,
    EpochMismatch { expected: u64, actual: u64 },
    LockLost,
    DrainTimeout,
    NotOwner,
}

impl fmt::Display for OwnershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Draining => write!(f, "Runtime is draining; cannot issue writer permits"),
            Self::EpochMismatch { expected, actual } => {
                write!(
                    f,
                    "Owner epoch mismatch: expected {expected}, held {actual}"
                )
            }
            Self::LockLost => write!(f, "Lock guard lost or invalid"),
            Self::DrainTimeout => {
                write!(
                    f,
                    "Timed out waiting for outstanding writer permits to drain"
                )
            }
            Self::NotOwner => {
                write!(f, "Process is a follower and cannot issue writer permits")
            }
        }
    }
}

impl std::error::Error for OwnershipError {}

/// Authentic ownership epoch backed by a held `DaemonLockGuard`.
/// Can only be constructed upon successful OS lock acquisition.
pub struct OwnerEpoch {
    epoch: u64,
    workspace_id: String,
    lock_guard: DaemonLockGuard,
    draining: AtomicBool,
    active_permits: AtomicUsize,
    drain_notify: Arc<Notify>,
}

impl OwnerEpoch {
    /// Internal constructor requiring an authentic OS lock guard.
    pub fn new(epoch: u64, workspace_id: String, lock_guard: DaemonLockGuard) -> Self {
        Self {
            epoch,
            workspace_id,
            lock_guard,
            draining: AtomicBool::new(false),
            active_permits: AtomicUsize::new(0),
            drain_notify: Arc::new(Notify::new()),
        }
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    pub fn lock_guard(&self) -> &DaemonLockGuard {
        &self.lock_guard
    }

    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::SeqCst)
    }

    pub fn mark_draining(&self) {
        self.draining.store(true, Ordering::SeqCst);
    }

    pub fn active_permits_count(&self) -> usize {
        self.active_permits.load(Ordering::SeqCst)
    }

    pub fn into_lock_guard(self) -> DaemonLockGuard {
        self.lock_guard
    }
}

struct PermitScopeGuard<'g> {
    epoch: &'g OwnerEpoch,
    active: bool,
}

impl<'g> Drop for PermitScopeGuard<'g> {
    fn drop(&mut self) {
        if self.active {
            self.epoch.active_permits.fetch_sub(1, Ordering::SeqCst);
            self.epoch.drain_notify.notify_waiters();
        }
    }
}

impl OwnerEpoch {
    /// Acquire a fenced, process-serialized writer permit from a borrowed epoch.
    pub async fn acquire_writer<'a>(
        &'a self,
        mutation_gate: &'a MutationGateRegistry,
    ) -> Result<WriterPermit<'a>, OwnershipError> {
        if self.draining.load(Ordering::SeqCst) {
            return Err(OwnershipError::Draining);
        }

        self.active_permits.fetch_add(1, Ordering::SeqCst);
        let mut cleanup = PermitScopeGuard {
            epoch: self,
            active: true,
        };

        if self.draining.load(Ordering::SeqCst) {
            return Err(OwnershipError::Draining);
        }

        let guard = mutation_gate.acquire(&self.workspace_id).await;

        if self.draining.load(Ordering::SeqCst) {
            drop(guard);
            return Err(OwnershipError::Draining);
        }

        cleanup.active = false;
        Ok(WriterPermit::new(self, guard))
    }

    /// Acquire a fenced, process-serialized writer permit from an Arc-owned epoch.
    pub async fn acquire_writer_shared<'a>(
        self: &Arc<Self>,
        mutation_gate: &'a MutationGateRegistry,
    ) -> Result<WriterPermit<'a>, OwnershipError> {
        if self.draining.load(Ordering::SeqCst) {
            return Err(OwnershipError::Draining);
        }

        self.active_permits.fetch_add(1, Ordering::SeqCst);
        let mut cleanup = PermitScopeGuard {
            epoch: self,
            active: true,
        };

        if self.draining.load(Ordering::SeqCst) {
            return Err(OwnershipError::Draining);
        }

        let guard = mutation_gate.acquire(&self.workspace_id).await;

        if self.draining.load(Ordering::SeqCst) {
            drop(guard);
            return Err(OwnershipError::Draining);
        }

        cleanup.active = false;
        Ok(WriterPermit::new_shared(Arc::clone(self), guard))
    }

    /// Stop new permits and wait for outstanding permits to drain before releasing the lock.
    pub async fn drain_permits(&self, timeout: Duration) -> Result<(), OwnershipError> {
        self.mark_draining();
        let deadline = tokio::time::Instant::now() + timeout;

        while self.active_permits.load(Ordering::SeqCst) > 0 {
            if tokio::time::Instant::now() >= deadline {
                return Err(OwnershipError::DrainTimeout);
            }
            tokio::select! {
                _ = self.drain_notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(15)) => {}
            }
        }

        Ok(())
    }
}

pub enum OwnerEpochRef<'a> {
    Borrowed(&'a OwnerEpoch),
    Shared(Arc<OwnerEpoch>),
}

impl<'a> Deref for OwnerEpochRef<'a> {
    type Target = OwnerEpoch;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(b) => b,
            Self::Shared(s) => s.as_ref(),
        }
    }
}

/// Fenced writer permit required by all 8 index writers across Julie.
pub struct WriterPermit<'a> {
    owner: OwnerEpochRef<'a>,
    mutation: MutationGuard<'a>,
}

impl<'a> WriterPermit<'a> {
    pub(crate) fn new(owner: &'a OwnerEpoch, mutation: MutationGuard<'a>) -> Self {
        Self {
            owner: OwnerEpochRef::Borrowed(owner),
            mutation,
        }
    }

    pub(crate) fn new_shared(owner: Arc<OwnerEpoch>, mutation: MutationGuard<'a>) -> Self {
        Self {
            owner: OwnerEpochRef::Shared(owner),
            mutation,
        }
    }

    pub fn epoch(&self) -> u64 {
        self.owner.epoch
    }

    pub fn workspace_id(&self) -> &str {
        &self.owner.workspace_id
    }

    pub fn mutation_guard(&self) -> &MutationGuard<'a> {
        &self.mutation
    }
}

impl<'a> Deref for WriterPermit<'a> {
    type Target = MutationGuard<'a>;
    fn deref(&self) -> &Self::Target {
        &self.mutation
    }
}

impl<'a> Drop for WriterPermit<'a> {
    fn drop(&mut self) {
        self.owner.active_permits.fetch_sub(1, Ordering::SeqCst);
        self.owner.drain_notify.notify_waiters();
    }
}

impl<'a> fmt::Debug for WriterPermit<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WriterPermit")
            .field("epoch", &self.owner.epoch)
            .field("workspace_id", &self.owner.workspace_id)
            .finish()
    }
}

/// Canonical kind/discriminant of runtime lifecycle phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimePhaseKind {
    /// Initial phase while acquiring resources and inspecting locks.
    Opening,
    /// Follower role: index reads and source edits allowed; index mutations refused.
    Follower,
    /// Recovery role: holding owner lock, reconciling projection lag, starting watcher.
    Recovering,
    /// Active index owner: exclusive writer for SQLite and Tantivy.
    Owner,
    /// Graceful shutdown: draining queued commits, stopping watcher, closing handles.
    Draining,
    /// Unrecoverable error state.
    Failed,
}

impl fmt::Display for RuntimePhaseKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Opening => write!(f, "Opening"),
            Self::Follower => write!(f, "Follower"),
            Self::Recovering => write!(f, "Recovering"),
            Self::Owner => write!(f, "Owner"),
            Self::Draining => write!(f, "Draining"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

/// The authoritative state transition guard for workspace runtime lifecycle.
///
/// Invariants:
/// - `Follower -> Recovering`: allowed.
/// - `Follower -> Owner`: forbidden (never skip recovery!).
/// - `Recovering -> Owner`: allowed.
/// - `Draining -> Owner`: forbidden.
/// - `Failed -> Owner`: forbidden.
#[inline]
pub fn allowed_transition(from: RuntimePhaseKind, to: RuntimePhaseKind) -> bool {
    match (from, to) {
        // Opening transitions
        (RuntimePhaseKind::Opening, RuntimePhaseKind::Follower) => true,
        (RuntimePhaseKind::Opening, RuntimePhaseKind::Recovering) => true,
        (RuntimePhaseKind::Opening, RuntimePhaseKind::Draining) => true,
        (RuntimePhaseKind::Opening, RuntimePhaseKind::Failed) => true,

        // Follower transitions
        (RuntimePhaseKind::Follower, RuntimePhaseKind::Recovering) => true,
        (RuntimePhaseKind::Follower, RuntimePhaseKind::Draining) => true,
        (RuntimePhaseKind::Follower, RuntimePhaseKind::Failed) => true,

        // Recovering transitions
        (RuntimePhaseKind::Recovering, RuntimePhaseKind::Owner) => true,
        (RuntimePhaseKind::Recovering, RuntimePhaseKind::Draining) => true,
        (RuntimePhaseKind::Recovering, RuntimePhaseKind::Failed) => true,

        // Owner transitions
        (RuntimePhaseKind::Owner, RuntimePhaseKind::Owner) => true,
        (RuntimePhaseKind::Owner, RuntimePhaseKind::Draining) => true,
        (RuntimePhaseKind::Owner, RuntimePhaseKind::Failed) => true,

        // Draining transitions
        (RuntimePhaseKind::Draining, RuntimePhaseKind::Failed) => true,

        // All other transitions are strictly forbidden
        _ => false,
    }
}
