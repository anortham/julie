//! Singleton daemon enforcement via an exclusive file lock.
//!
//! Only one process holding the exclusive `flock` on `daemon.singleton.lock`
//! may run as the daemon for a given JULIE_HOME. The lock is acquired at
//! the very start of daemon startup (before PID file creation) and held
//! for the daemon's lifetime; when the process exits, the kernel releases
//! the lock automatically.
//!
//! **Why a separate file from `daemon.pid`:** `flock` is bound to the
//! kernel's open-file-description, which is tied to the file's inode.
//! `PidFile::create_exclusive` may unlink + recreate the PID file on stale
//! recovery, which breaks flock semantics for the original holder (their
//! lock is now on an unlinked inode while a new daemon locks the new
//! inode). The singleton lock file is never unlinked; it lives at a stable
//! inode for the JULIE_HOME's lifetime so concurrent acquirers always
//! contend on the same kernel object.
//!
//! **Why not `daemon.lock`:** the adapter's launcher already holds an
//! exclusive flock on `daemon.lock` across `wait_for_daemon_ready`. The
//! daemon process is spawned by that same launcher, so reusing the path
//! would mean the daemon is racing against (or deadlocking with) the
//! adapter that just spawned it.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use fs2::FileExt;

/// Holds an exclusive `flock` on the singleton lock file. The lock is
/// released when this value is dropped (file closed → kernel releases).
#[derive(Debug)]
pub struct SingletonLock {
    file: File,
    path: PathBuf,
}

/// Failure modes for `SingletonLock::try_acquire`.
#[derive(Debug)]
pub enum SingletonLockError {
    /// Another process already holds the singleton lock for this path.
    AlreadyHeld { path: PathBuf },
    /// Filesystem or kernel error while opening or locking the file.
    Io { path: PathBuf, source: io::Error },
}

impl SingletonLock {
    /// Try to acquire an exclusive `flock` on the singleton lock file at
    /// `path`, creating it if it doesn't exist. Never truncates and never
    /// unlinks the file. Returns `AlreadyHeld` immediately if another
    /// process holds the lock — does not block.
    pub fn try_acquire(path: &Path) -> Result<Self, SingletonLockError> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|source| SingletonLockError::Io {
                path: path.to_path_buf(),
                source,
            })?;

        match FileExt::try_lock_exclusive(&file) {
            Ok(()) => Ok(Self {
                file,
                path: path.to_path_buf(),
            }),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                Err(SingletonLockError::AlreadyHeld {
                    path: path.to_path_buf(),
                })
            }
            Err(source) => Err(SingletonLockError::Io {
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

impl Drop for SingletonLock {
    fn drop(&mut self) {
        // Closing the file (when `self.file` drops) releases the flock at
        // the kernel level. Calling `unlock` explicitly makes the intent
        // unambiguous and surfaces any unlock error to the trace layer.
        if let Err(e) = FileExt::unlock(&self.file) {
            tracing::warn!(
                path = %self.path.display(),
                error = %e,
                "Failed to release singleton lock cleanly; file close will still release",
            );
        }
    }
}

impl std::fmt::Display for SingletonLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SingletonLockError::AlreadyHeld { path } => write!(
                f,
                "another daemon is already running for this JULIE_HOME (singleton lock held: {})",
                path.display()
            ),
            SingletonLockError::Io { path, source } => write!(
                f,
                "failed to acquire singleton lock at {}: {}",
                path.display(),
                source
            ),
        }
    }
}

impl std::error::Error for SingletonLockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SingletonLockError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
