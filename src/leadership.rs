//! In-process leader-election state for Phase 3c+.
//!
//! `LeadershipState` carries the OS-level advisory lock won during workspace
//! leader election. It is the seam T5/T7/T8/T10 build on for write-gating.
//!
//! The lock guard (`DaemonLockGuard`) is NOT `Clone`, so handlers store
//! `Arc<LeadershipState>` — every handler clone shares the same live guard.
//! The guard is released (and the kernel lock freed) when the last `Arc` drops.
//!
//! Three states:
//! - `leader(guard)` — won the election; owns all writes.
//! - `follower()`    — lost the election; pure reader, writes must be refused.
//! - `none()`        — not participating in any election (all pre-3c constructors).

use julie_core::workspace::leader_lock::DaemonLockGuard;
use julie_core::workspace::ownership::OwnerEpoch;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// In-process leadership state for a `JulieServerHandler`.
///
/// Create with [`LeadershipState::leader`] when this process won the lock,
/// [`LeadershipState::follower`] when it lost (must refuse D1 writes), or
/// [`LeadershipState::none`] for handlers that are not in the in-process model
/// (all existing pre-3c constructors use `none()`).
pub struct LeadershipState {
    pub(crate) lock: tokio::sync::Mutex<Option<DaemonLockGuard>>,
    pub(crate) owner_epoch: Arc<tokio::sync::RwLock<Option<Arc<OwnerEpoch>>>>,
    pub(crate) target_epochs:
        Arc<tokio::sync::RwLock<std::collections::HashMap<String, Arc<OwnerEpoch>>>>,
    is_leader: AtomicBool,
    /// `true` when this handler is participating in an in-process election
    /// (either as leader or as follower). `false` for regular non-in-process
    /// constructors that are not subject to write-refusal gating.
    in_process: bool,
    phase_rx: Option<tokio::sync::watch::Receiver<crate::workspace_runtime::RuntimePhase>>,
}

impl LeadershipState {
    /// Construct a leader state backed by an OS advisory lock guard.
    ///
    /// The guard is held for the lifetime of this value (and all `Arc` clones
    /// of it). Released automatically on drop.
    pub fn leader(guard: DaemonLockGuard) -> Self {
        Self {
            lock: tokio::sync::Mutex::new(Some(guard)),
            owner_epoch: Arc::new(tokio::sync::RwLock::new(None)),
            target_epochs: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            is_leader: AtomicBool::new(true),
            in_process: true,
            phase_rx: None,
        }
    }

    /// Construct a leader state directly initialized with an authentic `OwnerEpoch`.
    pub fn leader_with_epoch(epoch: Arc<OwnerEpoch>) -> Self {
        Self {
            lock: tokio::sync::Mutex::new(None),
            owner_epoch: Arc::new(tokio::sync::RwLock::new(Some(epoch))),
            target_epochs: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            is_leader: AtomicBool::new(true),
            in_process: true,
            phase_rx: None,
        }
    }

    /// Construct an in-process leader whose lock is held externally by the runtime manager.
    pub fn leader_in_process() -> Self {
        Self {
            lock: tokio::sync::Mutex::new(None),
            owner_epoch: Arc::new(tokio::sync::RwLock::new(None)),
            target_epochs: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            is_leader: AtomicBool::new(true),
            in_process: true,
            phase_rx: None,
        }
    }

    /// Construct an in-process follower: participated in the leader election
    /// but did not win. This handler is a pure reader — D1 write-mutating
    /// operations (index, register, remove, refresh, editing tools) must be
    /// refused to prevent cross-process SQLite/Tantivy data races (T7/Risk #2).
    pub fn follower() -> Self {
        Self {
            lock: tokio::sync::Mutex::new(None),
            owner_epoch: Arc::new(tokio::sync::RwLock::new(None)),
            target_epochs: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            is_leader: AtomicBool::new(false),
            in_process: true,
            phase_rx: None,
        }
    }

    /// Construct a dynamic leadership state linked to a runtime phase receiver.
    pub fn dynamic(
        phase_rx: tokio::sync::watch::Receiver<crate::workspace_runtime::RuntimePhase>,
    ) -> Self {
        Self {
            lock: tokio::sync::Mutex::new(None),
            owner_epoch: Arc::new(tokio::sync::RwLock::new(None)),
            target_epochs: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            is_leader: AtomicBool::new(false),
            in_process: true,
            phase_rx: Some(phase_rx),
        }
    }

    /// Construct a non-participating state (not in the in-process model).
    ///
    /// Used by all pre-3c constructors (`new`, `new_with_shared_workspace_*`,
    /// `new_deferred_daemon_startup_hint_*`). These handlers are NOT subject to
    /// the follower write-refusal gate.
    pub fn none() -> Self {
        Self {
            lock: tokio::sync::Mutex::new(None),
            owner_epoch: Arc::new(tokio::sync::RwLock::new(None)),
            target_epochs: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            is_leader: AtomicBool::new(false),
            in_process: false,
            phase_rx: None,
        }
    }

    /// Returns `true` when this process holds the workspace leader lock.
    pub fn is_leader(&self) -> bool {
        if let Some(ref rx) = self.phase_rx {
            rx.borrow().is_owner()
        } else {
            self.is_leader.load(Ordering::SeqCst)
        }
    }

    /// Returns `true` when this is an in-process participant that did NOT win
    /// the election. Write-mutating operations must be refused on followers.
    pub fn is_follower(&self) -> bool {
        if let Some(ref rx) = self.phase_rx {
            rx.borrow().is_follower()
        } else {
            self.in_process && !self.is_leader.load(Ordering::SeqCst)
        }
    }

    /// Returns `true` when this handler is participating in an in-process
    /// leader election (either as leader or as follower). `false` for all
    /// pre-3c constructors (`none()`) — daemon mode, stdio mode.
    ///
    /// Used to gate the F1 bounded in-process read envelope: only in-process
    /// handlers get the bounded envelope; daemon/stdio take the existing path
    /// byte-for-byte unchanged.
    pub fn is_in_process(&self) -> bool {
        self.in_process || self.phase_rx.is_some()
    }
}
