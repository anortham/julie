//! In-process leader-election state for Phase 3c+.
//!
//! `LeadershipState` carries the OS-level advisory lock won during workspace
//! leader election. It is the seam T5/T8/T10 build on for write-gating.
//!
//! The lock guard (`DaemonLockGuard`) is NOT `Clone`, so handlers store
//! `Arc<LeadershipState>` — every handler clone shares the same live guard.
//! The guard is released (and the kernel lock freed) when the last `Arc` drops.

use julie_core::workspace::leader_lock::DaemonLockGuard;

/// In-process leadership state for a `JulieServerHandler`.
///
/// Create with [`LeadershipState::leader`] when this process won the workspace
/// leader election, or with [`LeadershipState::none`] for follower / uncontested
/// (all existing constructors use `none()`).
pub struct LeadershipState {
    lock: Option<DaemonLockGuard>,
}

impl LeadershipState {
    /// Construct a leader state backed by an OS advisory lock guard.
    ///
    /// The guard is held for the lifetime of this value (and all `Arc` clones
    /// of it). Released automatically on drop.
    pub fn leader(guard: DaemonLockGuard) -> Self {
        Self { lock: Some(guard) }
    }

    /// Construct a non-leader (follower / uncontested) state.
    ///
    /// Used by all existing constructors (`new`, `new_with_shared_workspace_*`,
    /// `new_deferred_daemon_startup_hint_*`).
    pub fn none() -> Self {
        Self { lock: None }
    }

    /// Returns `true` when this process holds the workspace leader lock.
    pub fn is_leader(&self) -> bool {
        self.lock.is_some()
    }
}
