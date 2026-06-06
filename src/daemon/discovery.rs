//! Compatibility re-export for the in-process leader lock.
//!
//! The old `discovery.json` reader/writer surface was deleted in Phase 3d.3.
//! New in-process sessions coordinate through the workspace leader lock, not
//! daemon discovery files.

pub use julie_core::workspace::leader_lock::{AcquireError, DaemonLockGuard, LockAlreadyHeld};
