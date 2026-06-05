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

/// In-process leadership state for a `JulieServerHandler`.
///
/// Create with [`LeadershipState::leader`] when this process won the lock,
/// [`LeadershipState::follower`] when it lost (must refuse D1 writes), or
/// [`LeadershipState::none`] for handlers that are not in the in-process model
/// (all existing pre-3c constructors use `none()`).
pub struct LeadershipState {
    lock: Option<DaemonLockGuard>,
    /// `true` when this handler is participating in an in-process election
    /// (either as leader or as follower). `false` for regular non-in-process
    /// constructors that are not subject to write-refusal gating.
    in_process: bool,
}

impl LeadershipState {
    /// Construct a leader state backed by an OS advisory lock guard.
    ///
    /// The guard is held for the lifetime of this value (and all `Arc` clones
    /// of it). Released automatically on drop.
    pub fn leader(guard: DaemonLockGuard) -> Self {
        Self { lock: Some(guard), in_process: true }
    }

    /// Construct an in-process follower: participated in the leader election
    /// but did not win. This handler is a pure reader — D1 write-mutating
    /// operations (index, register, remove, refresh, editing tools) must be
    /// refused to prevent cross-process SQLite/Tantivy data races (T7/Risk #2).
    pub fn follower() -> Self {
        Self { lock: None, in_process: true }
    }

    /// Construct a non-participating state (not in the in-process model).
    ///
    /// Used by all pre-3c constructors (`new`, `new_with_shared_workspace_*`,
    /// `new_deferred_daemon_startup_hint_*`). These handlers are NOT subject to
    /// the follower write-refusal gate.
    pub fn none() -> Self {
        Self { lock: None, in_process: false }
    }

    /// Returns `true` when this process holds the workspace leader lock.
    pub fn is_leader(&self) -> bool {
        self.lock.is_some()
    }

    /// Returns `true` when this is an in-process participant that did NOT win
    /// the election. Write-mutating operations must be refused on followers.
    pub fn is_follower(&self) -> bool {
        self.in_process && self.lock.is_none()
    }

    /// Returns `true` when this handler is participating in an in-process
    /// leader election (either as leader or as follower). `false` for all
    /// pre-3c constructors (`none()`) — daemon mode, stdio mode.
    ///
    /// Used to gate the F1 bounded in-process read envelope: only in-process
    /// handlers get the bounded envelope; daemon/stdio take the existing path
    /// byte-for-byte unchanged.
    pub fn is_in_process(&self) -> bool {
        self.in_process
    }
}
