//! Tests for `daemon::singleton::SingletonLock` — kernel-enforced
//! single-daemon-per-JULIE_HOME invariant.
//!
//! These tests are the structural guard for P1 of the 577-daemon cascade
//! fix. They verify that the lock contends correctly between independent
//! file opens of the same path (which is how two daemon processes would
//! see it) and that the lock file is preserved across acquire/release
//! cycles (so the stable-inode property holds).

#[cfg(test)]
mod tests {
    use crate::daemon::singleton::{SingletonLock, SingletonLockError};
    use tempfile::TempDir;

    fn temp_lock_path() -> (TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.singleton.lock");
        (dir, path)
    }

    /// First acquirer wins on an uncontended path.
    #[test]
    fn test_acquire_succeeds_when_uncontended() {
        let (_dir, path) = temp_lock_path();

        let lock = SingletonLock::try_acquire(&path)
            .expect("first acquire must succeed on uncontended path");

        assert_eq!(lock.path(), path);
        assert!(
            path.exists(),
            "singleton lock file must exist after acquisition"
        );
    }

    /// Second acquirer of the same path returns AlreadyHeld while the
    /// first guard is alive. This is the kernel-enforced "only one
    /// daemon" invariant.
    #[test]
    fn test_second_acquire_fails_with_already_held() {
        let (_dir, path) = temp_lock_path();

        let _first = SingletonLock::try_acquire(&path).expect("first acquire must succeed");

        let result = SingletonLock::try_acquire(&path);

        match result {
            Err(SingletonLockError::AlreadyHeld { path: held_path }) => {
                assert_eq!(
                    held_path, path,
                    "AlreadyHeld must report the contended path"
                );
            }
            Err(other) => panic!(
                "expected SingletonLockError::AlreadyHeld, got: {:?} ({})",
                other, other
            ),
            Ok(_) => panic!(
                "second acquire must NOT succeed while the first guard is alive — \
                 if this passes, two daemons could run concurrently"
            ),
        }
    }

    /// After the first holder drops, a fresh acquire succeeds — confirms
    /// `Drop` releases the kernel lock (not just the Rust handle).
    #[test]
    fn test_acquire_succeeds_after_holder_drops() {
        let (_dir, path) = temp_lock_path();

        {
            let first = SingletonLock::try_acquire(&path).expect("first acquire must succeed");
            drop(first);
        }

        let second = SingletonLock::try_acquire(&path)
            .expect("re-acquire must succeed after the previous holder drops");

        assert_eq!(second.path(), path);
    }

    /// The lock file MUST persist across acquire / release cycles. If we
    /// ever unlink it, two new acquirers could each `open()` a different
    /// inode and both `flock()` successfully — the singleton invariant
    /// would be silently broken. Regression guard for that mistake.
    #[test]
    fn test_lock_file_persists_after_release() {
        let (_dir, path) = temp_lock_path();

        {
            let lock = SingletonLock::try_acquire(&path).expect("acquire must succeed");
            assert!(path.exists(), "file must exist while held");
            drop(lock);
        }

        assert!(
            path.exists(),
            "singleton lock file must NOT be removed when the guard drops; \
             unlinking it would break the stable-inode invariant that makes \
             flock contention reliable across processes"
        );
    }

    /// Even if many threads race on `try_acquire` against the same path,
    /// at most one succeeds at any given time. This is the in-process
    /// proxy for the across-process race we actually care about (the
    /// 577-daemon cascade scenario), since `flock` on Linux contends
    /// across open-file-descriptions regardless of which process opens
    /// them.
    #[test]
    fn test_concurrent_acquires_only_one_wins() {
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
                if let Ok(guard) = SingletonLock::try_acquire(&path) {
                    winners.fetch_add(1, Ordering::SeqCst);
                    // Hold the lock long enough that all other contenders
                    // observe the contention rather than briefly missing it.
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
            "exactly one thread must acquire the singleton lock under contention; got {}",
            total
        );
    }
}
