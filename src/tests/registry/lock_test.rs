//! Tests for `daemon::discovery::DaemonLockGuard` — the kernel-held advisory
//! lock that enforces "only one daemon process per JULIE_HOME" in the new
//! daemon-split architecture.
//!
//! These tests verify:
//! - First acquire succeeds on an uncontended path.
//! - Second concurrent acquire returns `LockAlreadyHeld` with the contended path.
//! - Dropping the guard releases the lock so a subsequent acquire succeeds.
//! - The lock file is NOT unlinked on release (stable-inode invariant).
//! - Many threads racing produce exactly one winner at a time.
//!
//! The lock is implemented via `fs2::FileExt::try_lock_exclusive`, which uses
//! `flock` on POSIX and `LockFileEx(LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY)`
//! on Windows. Both APIs are file-descriptor scoped, so the kernel releases
//! the lock when the process exits (clean or crash). The single-process tests
//! work cross-platform because:
//!   - POSIX `flock` contends across open-file-descriptions: two
//!     `File::open` calls in the same process get independent descriptions.
//!   - Windows `LockFileEx` is per-handle and per-process — `fs2` documents
//!     that a second `try_lock_exclusive` in the same process also fails.

#[cfg(test)]
mod tests {
    use crate::registry::discovery::{AcquireError, DaemonLockGuard, LockAlreadyHeld};
    use tempfile::TempDir;

    fn temp_lock_path() -> (TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.lock");
        (dir, path)
    }

    /// First acquirer wins on an uncontended path.
    #[test]
    fn test_daemon_lock_try_acquire_succeeds_when_unheld() {
        let (_dir, path) = temp_lock_path();

        let guard = DaemonLockGuard::try_acquire(&path)
            .expect("first acquire must succeed on uncontended path");

        assert_eq!(guard.path(), path);
        assert!(
            path.exists(),
            "daemon lock file must exist after acquisition"
        );
    }

    /// Second acquirer of the same path returns LockAlreadyHeld while the
    /// first guard is alive. Kernel-enforced singleton invariant.
    #[test]
    fn test_daemon_lock_try_acquire_fails_when_held() {
        let (_dir, path) = temp_lock_path();

        let _first = DaemonLockGuard::try_acquire(&path).expect("first acquire must succeed");

        let result = DaemonLockGuard::try_acquire(&path);

        match result {
            Err(AcquireError::AlreadyHeld(LockAlreadyHeld { path: held_path })) => {
                assert_eq!(
                    held_path, path,
                    "LockAlreadyHeld must report the contended path"
                );
            }
            Err(AcquireError::Io {
                path: io_path,
                source,
            }) => panic!(
                "expected AcquireError::AlreadyHeld, got Io error at {}: {}",
                io_path.display(),
                source
            ),
            Ok(_) => panic!(
                "second acquire must NOT succeed while the first guard is alive — \
                 if this passes, two daemons could run concurrently"
            ),
        }
    }

    /// Dropping the guard releases the kernel lock so a fresh acquire
    /// succeeds. Confirms `Drop` actually unlocks the descriptor, not just
    /// the Rust handle.
    #[test]
    fn test_daemon_lock_released_on_drop() {
        let (_dir, path) = temp_lock_path();

        {
            let first = DaemonLockGuard::try_acquire(&path).expect("first acquire must succeed");
            drop(first);
        }

        let second = DaemonLockGuard::try_acquire(&path)
            .expect("re-acquire must succeed after the previous holder drops");

        assert_eq!(second.path(), path);
    }

    /// The lock file MUST persist across acquire/release cycles. If we ever
    /// unlinked it on drop, two new acquirers could each `open()` a different
    /// inode and both succeed at the same time — the singleton invariant
    /// would silently break. Regression guard.
    #[test]
    fn test_daemon_lock_file_persists_after_release() {
        let (_dir, path) = temp_lock_path();

        {
            let guard = DaemonLockGuard::try_acquire(&path).expect("acquire must succeed");
            assert!(path.exists(), "file must exist while held");
            drop(guard);
        }

        assert!(
            path.exists(),
            "daemon lock file must NOT be removed when the guard drops; \
             unlinking it would break the stable-inode invariant that makes \
             kernel contention reliable across processes"
        );
    }

    /// Many threads racing on `try_acquire` against the same path: at most
    /// one wins at a time. In-process proxy for the across-process race
    /// (since both `flock` and `LockFileEx` contend across descriptions).
    #[test]
    fn test_daemon_lock_concurrent_acquires_only_one_wins() {
        use std::sync::{
            Arc, Barrier,
            atomic::{AtomicUsize, Ordering},
        };
        use std::thread;

        let (_dir, path) = temp_lock_path();
        let path = Arc::new(path);
        let barrier = Arc::new(Barrier::new(8));
        let winners = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            let winners = Arc::clone(&winners);
            handles.push(thread::spawn(move || {
                barrier.wait();
                if let Ok(guard) = DaemonLockGuard::try_acquire(&path) {
                    winners.fetch_add(1, Ordering::SeqCst);
                    // Hold long enough for the other contenders to observe
                    // contention rather than briefly missing it.
                    std::thread::sleep(std::time::Duration::from_millis(25));
                    drop(guard);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let total = winners.load(Ordering::SeqCst);
        assert_eq!(
            total, 1,
            "exactly one thread must acquire the daemon lock under contention; got {}",
            total
        );
    }
}
