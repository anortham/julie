//! Daemon discovery primitives for the daemon-split architecture.
//!
//! This module hosts the kernel-held advisory lock that enforces
//! "only one daemon process per JULIE_HOME". The discovery file
//! (`daemon-mcp-transport.json` identity validation) is added in A1.3.
//!
//! ## DaemonLockGuard
//!
//! `DaemonLockGuard` owns a long-lived file descriptor on `daemon.lock`
//! with an OS-native advisory lock acquired non-blockingly:
//!
//!   - **POSIX (macOS / Linux)**: `flock(LOCK_EX | LOCK_NB)` via
//!     `fs2::FileExt::try_lock_exclusive`. Advisory — only cooperating
//!     processes see it. The lock is tied to the open-file-description,
//!     so the kernel releases it on process exit (clean or crash).
//!   - **Windows**: `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK |
//!     LOCKFILE_FAIL_IMMEDIATELY)` via the same `fs2` wrapper.
//!     Mandatory at the handle level; released by the kernel on
//!     process exit.
//!
//! `fs2` was chosen over direct `nix`/`rustix`/`windows-sys` calls
//! because Julie already depends on it and `src/daemon/singleton.rs`
//! is the proven precedent for the same pattern in this codebase.
//! The plan-specified invariants — drop releases, kernel releases on
//! crash, second concurrent acquire fails — all hold with the `fs2`
//! wrapper exactly as they would with the direct syscalls.
//!
//! ## Lock file persistence
//!
//! The lock file at `~/.julie/daemon.lock` persists across daemon
//! lifetimes; only the lock state itself is per-process. We never
//! truncate, unlink, or recreate the file. Unlinking would break the
//! stable-inode invariant that makes `flock` contention reliable
//! across processes.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use fs2::FileExt;

/// Holds an exclusive OS-native advisory lock on the daemon lock file.
///
/// On `Drop`, the lock is released by closing the file (the kernel
/// releases the descriptor-bound lock automatically). We also call
/// `unlock` explicitly so any error surfaces in the trace layer.
///
/// The lock file is NOT unlinked on drop — see module docs.
#[derive(Debug)]
pub struct DaemonLockGuard {
    file: File,
    path: PathBuf,
}

/// Error returned when another holder already owns the daemon lock for
/// the given path. Distinct from any other I/O error so callers can
/// take the "another daemon is running" branch unambiguously.
#[derive(Debug)]
pub struct LockAlreadyHeld {
    pub path: PathBuf,
}

impl DaemonLockGuard {
    /// Try to acquire an exclusive OS-native advisory lock on
    /// `path`, creating the file if it doesn't exist. Never truncates
    /// and never unlinks.
    ///
    /// Returns `Err(LockAlreadyHeld)` immediately if another holder has
    /// the lock — does not block. Other filesystem or kernel failures
    /// (e.g. EACCES, permission denied, parent dir missing) are
    /// reported via `io::Error`.
    pub fn try_acquire(path: &Path) -> Result<Self, AcquireError> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|source| AcquireError::Io {
                path: path.to_path_buf(),
                source,
            })?;

        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Self {
                file,
                path: path.to_path_buf(),
            }),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                Err(AcquireError::AlreadyHeld(LockAlreadyHeld {
                    path: path.to_path_buf(),
                }))
            }
            Err(source) => Err(AcquireError::Io {
                path: path.to_path_buf(),
                source,
            }),
        }
    }

    /// Path of the lock file this guard holds.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for DaemonLockGuard {
    fn drop(&mut self) {
        // Closing the file (when `self.file` drops) releases the
        // descriptor-bound lock at the kernel level. Calling `unlock`
        // explicitly makes the intent unambiguous and surfaces any
        // unlock error through the trace layer.
        if let Err(e) = FileExt::unlock(&self.file) {
            tracing::warn!(
                path = %self.path.display(),
                error = %e,
                "Failed to release daemon lock cleanly; file close will still release",
            );
        }
    }
}

/// Full failure surface for `DaemonLockGuard::try_acquire`.
///
/// Callers that only care about the "another daemon is running"
/// outcome can match on `AcquireError::AlreadyHeld(_)`. Tests use
/// `From<AcquireError>` to peel the `LockAlreadyHeld` variant directly
/// — see the conversion impl below.
#[derive(Debug)]
pub enum AcquireError {
    /// Another holder owns the lock for `path`. Carries the typed
    /// error so callers can propagate it without re-wrapping.
    AlreadyHeld(LockAlreadyHeld),
    /// Filesystem or kernel error while opening or locking the file.
    Io { path: PathBuf, source: io::Error },
}

impl From<LockAlreadyHeld> for AcquireError {
    fn from(value: LockAlreadyHeld) -> Self {
        AcquireError::AlreadyHeld(value)
    }
}

impl std::fmt::Display for LockAlreadyHeld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "another daemon already holds the lock at {}",
            self.path.display()
        )
    }
}

impl std::error::Error for LockAlreadyHeld {}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcquireError::AlreadyHeld(held) => write!(f, "{}", held),
            AcquireError::Io { path, source } => write!(
                f,
                "failed to acquire daemon lock at {}: {}",
                path.display(),
                source
            ),
        }
    }
}

impl std::error::Error for AcquireError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AcquireError::Io { source, .. } => Some(source),
            AcquireError::AlreadyHeld(_) => None,
        }
    }
}
