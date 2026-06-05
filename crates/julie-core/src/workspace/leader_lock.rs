//! OS-native advisory file lock for workspace leader election.
//!
//! Provides `DaemonLockGuard`, an exclusive advisory lock backed by
//! `fs2::FileExt::try_lock_exclusive` (flock on POSIX, LockFileEx on Windows).
//! The in-process dedup via `HELD_DAEMON_LOCKS` ensures a single process can
//! hold at most one guard per canonical path, making it usable as a
//! leader-election primitive both for the daemon singleton and, in Phase 3c+,
//! for per-workspace leaders.
//!
//! ## Lock file persistence
//!
//! The lock file persists across lifetimes; only the lock state itself is
//! per-process. We never truncate, unlink, or recreate the file. Unlinking
//! would break the stable-inode invariant that makes `flock` contention
//! reliable across processes.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use fs2::FileExt;

static HELD_DAEMON_LOCKS: OnceLock<Mutex<std::collections::HashSet<PathBuf>>> = OnceLock::new();

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
    lock_key: PathBuf,
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
        let lock_key = normalize_lock_path(path);
        let mut held_locks = held_daemon_locks()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if held_locks.contains(&lock_key) {
            return Err(AcquireError::AlreadyHeld(LockAlreadyHeld {
                path: path.to_path_buf(),
            }));
        }

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
            Ok(()) => {
                held_locks.insert(lock_key.clone());
                Ok(Self {
                    file,
                    path: path.to_path_buf(),
                    lock_key,
                })
            }
            Err(e) if is_lock_already_held(&e) => Err(AcquireError::AlreadyHeld(LockAlreadyHeld {
                path: path.to_path_buf(),
            })),
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
        held_daemon_locks()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.lock_key);
    }
}

fn held_daemon_locks() -> &'static Mutex<std::collections::HashSet<PathBuf>> {
    HELD_DAEMON_LOCKS.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

fn normalize_lock_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn is_lock_already_held(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock || cfg!(windows) && error.raw_os_error() == Some(33)
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

#[cfg(test)]
mod tests {
    use super::{AcquireError, DaemonLockGuard};

    /// Proves the in-process dedup invariant:
    /// - two `try_acquire` calls on the same path yield `AcquireError::AlreadyHeld`
    /// - dropping the first guard releases the key so a third `try_acquire` succeeds.
    #[test]
    fn test_leader_lock_in_process_dedup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.lock");

        // First acquire must succeed on an uncontended path.
        let first = DaemonLockGuard::try_acquire(&path)
            .expect("first acquire must succeed on uncontended path");

        // Second acquire of same path must fail with AlreadyHeld — in-process dedup fires
        // before touching the kernel lock.
        let result = DaemonLockGuard::try_acquire(&path);
        assert!(
            matches!(result, Err(AcquireError::AlreadyHeld(_))),
            "second try_acquire on held path must return AlreadyHeld; got: {:?}",
            result.err()
        );

        // Dropping the first guard releases the in-process dedup key.
        drop(first);

        // Third acquire must now succeed — the dedup key was removed on drop.
        let _third = DaemonLockGuard::try_acquire(&path)
            .expect("acquire after drop must succeed — dedup key released");
    }
}
