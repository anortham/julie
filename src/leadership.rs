//! In-process writer state for a `JulieServerHandler`.
//!
//! `LeadershipState` carries the OS-level advisory lock for the workspace this
//! handler writes. Every in-process handler is the writer; there is no follower.
//!
//! The lock guard (`DaemonLockGuard`) is NOT `Clone`, so handlers store
//! `Arc<LeadershipState>` — every handler clone shares the same live guard.
//! The guard is released (and the kernel lock freed) when the last `Arc` drops.

use julie_core::workspace::leader_lock::DaemonLockGuard;
use julie_core::workspace::ownership::OwnerEpoch;
use std::sync::Arc;

/// In-process writer state for a `JulieServerHandler`.
///
/// Create with [`LeadershipState::leader`] when the handler holds the lock guard,
/// [`LeadershipState::leader_in_process`] when the runtime manager holds it, or
/// [`LeadershipState::none`] for handlers that are not in the in-process model.
pub struct LeadershipState {
    pub(crate) lock: tokio::sync::Mutex<Option<DaemonLockGuard>>,
    pub(crate) owner_epoch: Arc<tokio::sync::RwLock<Option<Arc<OwnerEpoch>>>>,
    pub(crate) target_epochs:
        Arc<tokio::sync::RwLock<std::collections::HashMap<String, Arc<OwnerEpoch>>>>,
    /// `true` when this handler was built by an in-process constructor.
    in_process: bool,
}

impl LeadershipState {
    fn with_lock(lock: Option<DaemonLockGuard>, in_process: bool) -> Self {
        Self {
            lock: tokio::sync::Mutex::new(lock),
            owner_epoch: Arc::new(tokio::sync::RwLock::new(None)),
            target_epochs: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            in_process,
        }
    }

    /// Construct a writer state backed by an OS advisory lock guard.
    ///
    /// The guard is held for the lifetime of this value (and all `Arc` clones
    /// of it). Released automatically on drop.
    pub fn leader(guard: DaemonLockGuard) -> Self {
        Self::with_lock(Some(guard), true)
    }

    /// Construct an in-process writer whose lock is held externally by the runtime manager.
    pub fn leader_in_process() -> Self {
        Self::with_lock(None, true)
    }

    /// Construct a non-participating state (not in the in-process model).
    ///
    /// Used by all pre-3c constructors (`new`, `new_with_shared_workspace_*`,
    /// `new_deferred_daemon_startup_hint_*`).
    pub fn none() -> Self {
        Self::with_lock(None, false)
    }

    /// Returns `true` when this handler was built by an in-process constructor.
    ///
    /// Used to gate the F1 bounded in-process read envelope: only in-process
    /// handlers get the bounded envelope; daemon/stdio take the existing path
    /// byte-for-byte unchanged.
    pub fn is_in_process(&self) -> bool {
        self.in_process
    }
}
