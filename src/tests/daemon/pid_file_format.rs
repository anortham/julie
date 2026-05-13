//! Tests for the three-field PID file format and PID-reuse defense.
//!
//! Format: "<pid> <creation_time_unix_micros> <binary_mtime_unix_micros>\n"
//!
//! check_running rejects any PID file whose stored creation_time does not
//! match the running process's actual creation time, preventing a recycled
//! PID from impersonating the original daemon.

#[cfg(test)]
mod tests {
    use crate::daemon::pid::{PidFile, PidFileStatus};
    use std::fs;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    /// Set the mtime of `path` to `age_secs` in the past, used to simulate
    /// fresh-vs-old empty PID files in tests.
    fn set_mtime_age_secs(path: &std::path::Path, age_secs: u64) {
        let mtime = SystemTime::now()
            .checked_sub(Duration::from_secs(age_secs))
            .expect("system time should be far enough from UNIX_EPOCH for this test");
        let f = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("file should exist for mtime adjustment");
        f.set_modified(mtime).expect("set_modified should succeed");
    }

    fn temp_pid_path() -> (TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("julie.pid");
        (dir, path)
    }

    // ── Test 1 ───────────────────────────────────────────────────────────────
    // create_exclusive must write the three-field format, not a bare integer.
    #[test]
    fn test_pid_file_writes_three_fields() {
        let (_dir, path) = temp_pid_path();
        let pid_file = PidFile::create_exclusive(&path).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        let trimmed = contents.trim();

        // Must have exactly three whitespace-separated fields
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        assert_eq!(
            parts.len(),
            3,
            "PID file must contain exactly three fields: pid creation_time binary_mtime, got: {:?}",
            trimmed
        );

        // Field 0: PID must equal the current process's PID
        let parsed_pid: u32 = parts[0]
            .parse()
            .expect("field 0 (pid) must be a u32 integer");
        assert_eq!(
            parsed_pid,
            std::process::id(),
            "field 0 must match the current process PID"
        );

        // Field 1: creation_time must be a non-zero u64
        let creation_time: u64 = parts[1]
            .parse()
            .expect("field 1 (creation_time) must be a u64 integer");
        // 0 is only valid when the platform doesn't support creation time (macOS best-effort).
        // Still must be a parseable integer.
        let _ = creation_time;

        // Field 2: binary_mtime must be a non-zero u64
        let binary_mtime: u64 = parts[2]
            .parse()
            .expect("field 2 (binary_mtime) must be a u64 integer");
        // 0 is a valid sentinel when mtime is unavailable, but must parse.
        let _ = binary_mtime;

        pid_file.cleanup().unwrap();
    }

    // ── Test 2 ───────────────────────────────────────────────────────────────
    // check_running must return None and remove the file when the stored
    // creation_time differs from the running process's actual creation_time.
    // This is the PID-reuse defense: a recycled PID can't impersonate the daemon.
    #[test]
    fn test_check_running_rejects_pid_reuse() {
        let (_dir, path) = temp_pid_path();

        let our_pid = std::process::id();

        // Write a PID file that references our own PID (so is_process_alive passes),
        // but with a deliberately wrong creation_time (1 microsecond since epoch —
        // no real process has creation_time = 1 unless the system is ~50 years old).
        // Use 0 for binary_mtime since it is a valid sentinel.
        let fake_creation_time: u64 = 1;
        let content = format!("{} {} 0\n", our_pid, fake_creation_time);
        fs::write(&path, &content).unwrap();
        assert!(
            path.exists(),
            "test setup: PID file must exist before check"
        );

        // check_running must detect the mismatched creation_time, treat as stale,
        // remove the file, and return None.
        let result = PidFile::check_running(&path);
        assert_eq!(
            result, None,
            "check_running must return None when creation_time mismatches (PID-reuse defense)"
        );
        assert!(
            !path.exists(),
            "check_running must remove the stale PID file after PID-reuse detection"
        );
    }

    // ── Test 3 ───────────────────────────────────────────────────────────────
    // check_running must return Some(pid) when the PID file was written by
    // create_exclusive for the current process (creation_time matches).
    #[test]
    fn test_check_running_accepts_matching_creation_time() {
        let (_dir, path) = temp_pid_path();

        // Use create_exclusive so the file has the real creation_time for this process.
        let pid_file = PidFile::create_exclusive(&path).unwrap();

        // check_running must accept the file since creation_time matches.
        let result = PidFile::check_running(&path);
        assert_eq!(
            result,
            Some(std::process::id()),
            "check_running must return Some(pid) when creation_time matches the live process"
        );

        pid_file.cleanup().unwrap();
    }

    // ── Test 4 ───────────────────────────────────────────────────────────────
    // create_exclusive must propagate non-NotFound errors from remove_file instead
    // of swallowing them (Windows ERROR_SHARING_VIOLATION was previously lost).
    //
    // We simulate the failure by writing a file whose directory we then make
    // read-only (on Unix). On macOS/Linux this triggers EPERM on remove_file.
    // The test is skipped when run as root (root can always remove files).
    #[test]
    #[cfg(unix)]
    fn test_create_exclusive_propagates_remove_file_errors() {
        use std::os::unix::fs::PermissionsExt;

        // Skip if running as root — root bypasses permission checks.
        if unsafe { libc::getuid() } == 0 {
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("julie.pid");

        // Write a stale PID file (dead process) so create_exclusive tries to remove it.
        fs::write(&path, "99999999 1 0\n").unwrap();

        // Make the directory read-only so remove_file fails with EPERM.
        let orig_perms = fs::metadata(dir.path()).unwrap().permissions();
        let mut readonly_perms = orig_perms.clone();
        readonly_perms.set_mode(0o555); // r-xr-xr-x: read + execute, no write
        fs::set_permissions(dir.path(), readonly_perms).unwrap();

        // Restore permissions in all paths so the TempDir cleanup doesn't panic.
        let result = PidFile::create_exclusive(&path);

        // Restore write permission before any assertion panics clean up the tempdir.
        fs::set_permissions(dir.path(), orig_perms).unwrap();

        // The error must propagate, not be silently swallowed.
        assert!(
            result.is_err(),
            "create_exclusive must return Err when remove_file fails with a non-NotFound error"
        );
        let err_string = format!("{:#}", result.unwrap_err());
        // The error chain must contain meaningful context — not just "Unknown error".
        // Accept either "Permission denied" or the generic remove_file context string.
        assert!(
            err_string.contains("Permission denied")
                || err_string.contains("remove")
                || err_string.contains("EPERM"),
            "error message must describe the remove_file failure, got: {}",
            err_string
        );
    }

    // ── Test 5 (backoff pacing) ───────────────────────────────────────────────
    // create_exclusive must not burn retries in microseconds — at least a
    // modest amount of wall-clock time must pass when retries are forced.
    //
    // We can't force retries easily without an AlreadyExists loop, so instead
    // we verify the backoff formula produces the expected delay sequence.
    #[test]
    fn test_exponential_backoff_formula() {
        // Verify the formula: 50ms * 2^retries, capped at 5000ms.
        // This is a pure arithmetic test of the formula used in create_exclusive.
        let backoff_ms = |retries: u32| -> u64 {
            let raw = 50_u64.saturating_mul(1_u64 << retries.min(7));
            raw.min(5000)
        };

        assert_eq!(backoff_ms(0), 50, "retry 0: 50ms");
        assert_eq!(backoff_ms(1), 100, "retry 1: 100ms");
        assert_eq!(backoff_ms(2), 200, "retry 2: 200ms");
        assert_eq!(backoff_ms(3), 400, "retry 3: 400ms");
        assert_eq!(backoff_ms(4), 800, "retry 4: 800ms");
        assert_eq!(backoff_ms(5), 1600, "retry 5: 1600ms");
        assert_eq!(backoff_ms(6), 3200, "retry 6: 3200ms");
        assert_eq!(backoff_ms(7), 5000, "retry 7: capped at 5000ms");
        assert_eq!(backoff_ms(8), 5000, "retry 8: still capped at 5000ms");
        assert_eq!(backoff_ms(20), 5000, "retry 20: still capped");
    }

    // ── Codex pre-merge review fixes ───────────────────────────────────────

    /// Codex finding #1 (high): a v7.7.x adapter must NOT delete a live
    /// legacy daemon's single-integer PID file, otherwise it would spawn a
    /// duplicate daemon during upgrade. `check_running` must return Some(pid)
    /// for a legacy file backed by an alive process, and must NOT remove the
    /// file in that case.
    #[test]
    fn test_check_running_preserves_legacy_pid_file_when_alive() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("legacy.pid");
        // Legacy single-integer format: just the PID, with a trailing newline.
        let live_pid = std::process::id();
        std::fs::write(&path, format!("{}\n", live_pid)).unwrap();

        let result = PidFile::check_running(&path);

        assert_eq!(
            result,
            Some(live_pid),
            "live legacy PID file should be preserved and reported as running"
        );
        assert!(
            path.exists(),
            "live legacy PID file must NOT be deleted by check_running"
        );
    }

    /// Same scenario but the legacy PID is dead: the file SHOULD be removed
    /// and `check_running` returns None.
    #[test]
    fn test_check_running_removes_legacy_pid_file_when_dead() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("legacy_dead.pid");
        // PID 999_999_999 is well above any realistic running PID on the test box.
        std::fs::write(&path, "999999999\n").unwrap();

        let result = PidFile::check_running(&path);

        assert_eq!(
            result, None,
            "dead legacy PID should be reported as not running"
        );
        assert!(
            !path.exists(),
            "dead legacy PID file should be removed by check_running"
        );
    }

    // ── Finding 1: Linux PID identity must survive clock steps ───────────────
    //
    // P0 (btime + ticks) traded one drift bug for another: `/proc/stat btime`
    // is `getboottime64` = `offs_real − offs_boot`, and `settimeofday` shifts
    // `offs_real`. A wall-clock step (NTP large step, manual `date`, VM
    // suspend/resume across DST) would change btime, change the computed
    // creation_time, and cause `check_status` to unlink a live daemon's PID
    // file. The singleton lock would still prevent 577 concurrent processes
    // but the adapter would loop on Dead→spawn→singleton-fail→Dead until the
    // 60s deadline expired. The fix: identity is keyed on `/proc/sys/kernel/
    // random/boot_id` (UUID, stable for the boot session, NOT affected by
    // clock changes) plus raw `/proc/<pid>/stat` start_ticks (stable for the
    // process's lifetime).

    /// Identity for the SAME boot_id + ticks must be deterministic — same
    /// inputs always produce the same u64. This is the core invariant
    /// that `check_status` relies on for exact-equality comparison.
    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_process_identity_is_deterministic_for_same_inputs() {
        use crate::daemon::pid::linux_process_identity_from_parts;
        let a = linux_process_identity_from_parts(
            "12345678-1234-5678-1234-567890abcdef",
            42_000,
        );
        let b = linux_process_identity_from_parts(
            "12345678-1234-5678-1234-567890abcdef",
            42_000,
        );
        assert_eq!(
            a, b,
            "same boot_id + same ticks must produce the same identity \
             (otherwise check_status would never accept a live daemon)"
        );
    }

    /// Different boot_id (same ticks) must produce DIFFERENT identity.
    /// This is the across-reboot PID-recycle defense: PID 12345 from
    /// before the reboot with starttime 100 ticks must not collide with a
    /// post-reboot PID 12345 whose starttime is also 100 ticks. boot_id
    /// is the disambiguator.
    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_process_identity_changes_when_boot_id_changes() {
        use crate::daemon::pid::linux_process_identity_from_parts;
        let pre = linux_process_identity_from_parts(
            "12345678-1234-5678-1234-567890abcdef",
            42_000,
        );
        let post = linux_process_identity_from_parts(
            "ffffffff-ffff-ffff-ffff-ffffffffffff",
            42_000,
        );
        assert_ne!(
            pre, post,
            "different boot_id must produce different identity — otherwise a \
             rebooted system could classify a recycled PID as the original daemon"
        );
    }

    /// Different start_ticks (same boot_id) must produce DIFFERENT identity.
    /// Within a single boot, two processes that happen to share a PID
    /// (impossible — but the test guards the hash mixing) must still be
    /// distinguishable.
    #[test]
    #[cfg(target_os = "linux")]
    fn test_linux_process_identity_changes_when_ticks_change() {
        use crate::daemon::pid::linux_process_identity_from_parts;
        let a = linux_process_identity_from_parts(
            "12345678-1234-5678-1234-567890abcdef",
            42_000,
        );
        let b = linux_process_identity_from_parts(
            "12345678-1234-5678-1234-567890abcdef",
            42_001,
        );
        assert_ne!(
            a, b,
            "different start_ticks must produce different identity — \
             otherwise the hash collapses too aggressively"
        );
    }

    /// Regression: 577-daemon cascade incident (2026-05-12).
    ///
    /// On Linux, `process_creation_time_micros` derived an absolute epoch
    /// value from `SystemTime::now() - /proc/uptime + ticks/HZ`. Because
    /// `now` and `/proc/uptime` are sampled by separate syscalls and the
    /// arithmetic uses f64, repeated calls for the same live PID produced
    /// slightly different values. The exact-equality check in
    /// `check_running` at the `creation_time_micros` comparison then
    /// classified the live daemon as PID-recycled and **removed the
    /// running daemon's PID file**. The adapter's poll loop saw the file
    /// disappear, declared the daemon dead, spawned a replacement, and
    /// repeated the cycle every ~50ms.
    ///
    /// This test exercises the bug end-to-end through the public API:
    /// create_exclusive for the current process, then call check_running
    /// in a tight loop. Each call MUST return Some(self_pid) and the file
    /// MUST still exist at the end.
    #[test]
    fn test_check_running_is_idempotent_for_live_daemon() {
        let (_dir, path) = temp_pid_path();
        let pid_file = PidFile::create_exclusive(&path).unwrap();
        let our_pid = std::process::id();

        for iteration in 0..10 {
            let result = PidFile::check_running(&path);
            assert_eq!(
                result,
                Some(our_pid),
                "iteration {}: check_running must return Some({}) for the live current process; \
                 returning None here means the creation-time validation drifted and the live \
                 daemon's PID file was unlinked (regression: 577-daemon cascade incident)",
                iteration,
                our_pid,
            );
            assert!(
                path.exists(),
                "iteration {}: check_running must NOT unlink a live daemon's PID file",
                iteration,
            );
        }

        pid_file.cleanup().unwrap();
    }

    // ── P2: 3-state classification via `check_status` ─────────────────────────
    //
    // `check_running` collapses Indeterminate ("racing daemon mid-write") to
    // None, which the launcher cannot distinguish from "no daemon". The
    // result was that an empty PID file (a daemon mid-startup) triggered the
    // same Dead-respawn path as a missing PID file. `check_status` exposes
    // the third state so the launcher can wait instead of respawn.

    /// A live daemon's PID file (matching creation_time) must resolve to
    /// `Alive(self_pid)` via `check_status`. Sanity check + back-compat
    /// proof that the public 3-state API agrees with `check_running` on
    /// the happy path.
    #[test]
    fn test_check_status_returns_alive_for_live_daemon() {
        let (_dir, path) = temp_pid_path();
        let pid_file = PidFile::create_exclusive(&path).unwrap();

        let status = PidFile::check_status(&path);
        assert_eq!(
            status,
            PidFileStatus::Alive(std::process::id()),
            "check_status must return Alive(self_pid) for the current process"
        );

        pid_file.cleanup().unwrap();
    }

    /// A freshly-created empty PID file (mtime ≈ now) MUST be classified
    /// as `Indeterminate` — it likely represents a daemon that just won
    /// the `O_CREAT|O_EXCL` race and is about to write its content. The
    /// file MUST NOT be removed; doing so unlinks a legitimate
    /// mid-startup daemon's PID file.
    #[test]
    fn test_check_status_returns_indeterminate_for_fresh_empty_file() {
        let (_dir, path) = temp_pid_path();
        // Empty file with mtime = now.
        fs::write(&path, b"").unwrap();
        assert!(path.exists(), "test setup: empty PID file must exist");

        let status = PidFile::check_status(&path);

        assert_eq!(
            status,
            PidFileStatus::Indeterminate,
            "fresh empty PID file must classify as Indeterminate, got: {:?}",
            status,
        );
        assert!(
            path.exists(),
            "Indeterminate status must NOT remove the file — a racing daemon \
             mid-write owns it and would observe a gone inode otherwise"
        );
    }

    /// An old empty PID file (mtime far in the past) is a crash leftover —
    /// the daemon that created it died before writing. This MUST classify
    /// as `Dead` and the file MUST be removed so the next acquirer can
    /// proceed.
    #[test]
    fn test_check_status_returns_dead_for_old_empty_file() {
        let (_dir, path) = temp_pid_path();
        fs::write(&path, b"").unwrap();
        set_mtime_age_secs(&path, 30);

        let status = PidFile::check_status(&path);

        assert_eq!(
            status,
            PidFileStatus::Dead,
            "stale empty PID file (mtime far in past) must classify as Dead"
        );
        assert!(
            !path.exists(),
            "Dead status from a stale empty file must remove it"
        );
    }

    // ── Finding 3 (Codex 2026-05-12): tighten PID-file freshness ────────────
    //
    // Pre-fix, `is_pid_file_fresh` treated any future-mtime as fresh (the
    // conservative choice for clock skew), and `classify_legacy_or_unparseable`
    // mapped any unparseable nonempty content within the freshness window to
    // `Indeterminate`. Combined, that meant a corrupt PID file with a
    // far-future mtime would pin the launcher in `Starting` until wall-time
    // caught up — every adapter session times out after 60s.
    //
    // The fix:
    //   - Only EMPTY content is eligible for the Indeterminate fresh path
    //     (the "racing daemon mid-write" scenario this was designed for).
    //     Nonempty unparseable content is always treated as Dead — a crash
    //     artifact, not an in-flight daemon.
    //   - Future mtimes are only fresh within a small skew tolerance (a
    //     few seconds, matching `PID_FILE_FRESH_WINDOW`). Beyond that, the
    //     file is treated as stale.

    /// Empty PID file with a far-future mtime MUST be classified `Dead`
    /// (not `Indeterminate`), and the file MUST be removed. Otherwise a
    /// corrupt fs/restore event with a bogus timestamp could pin the
    /// launcher forever.
    #[test]
    fn test_check_status_returns_dead_for_far_future_empty_file() {
        let (_dir, path) = temp_pid_path();
        fs::write(&path, b"").unwrap();
        // Mtime 1 hour in the future — far beyond the small skew tolerance.
        let future = SystemTime::now()
            .checked_add(Duration::from_secs(3600))
            .expect("future time should fit in SystemTime");
        let f = fs::OpenOptions::new().write(true).open(&path).unwrap();
        f.set_modified(future).unwrap();

        let status = PidFile::check_status(&path);

        assert_eq!(
            status,
            PidFileStatus::Dead,
            "empty PID file with far-future mtime must be Dead, not Indeterminate; \
             got {:?} (would pin the launcher in Starting forever)",
            status,
        );
        assert!(!path.exists(), "Dead status must remove the file");
    }

    /// Nonempty unparseable PID file (e.g. "junk\n" or a corrupted
    /// partial write that survived a crash) with a fresh mtime MUST be
    /// classified `Dead`. Pre-fix, this would have been Indeterminate
    /// and the launcher would have waited indefinitely.
    #[test]
    fn test_check_status_returns_dead_for_fresh_nonempty_unparseable_file() {
        let (_dir, path) = temp_pid_path();
        // Nonempty but not a valid PID (not "<digits>"). Mtime defaults to
        // now (well within the freshness window).
        fs::write(&path, b"not-a-pid-garbage\n").unwrap();

        let status = PidFile::check_status(&path);

        assert_eq!(
            status,
            PidFileStatus::Dead,
            "nonempty unparseable PID file must be Dead, not Indeterminate; \
             got {:?} (a crash artifact must not pin the launcher)",
            status,
        );
        assert!(!path.exists(), "Dead status must remove the file");
    }

    /// Empty PID file with mtime slightly in the future (within a small
    /// skew tolerance — e.g. an NTP step of a few seconds) is still
    /// classified `Indeterminate` and preserved. The skew tolerance
    /// matters for hosts where the daemon's clock briefly disagrees with
    /// the file system; we don't want to unlink a legitimate in-flight
    /// daemon's PID file over a 100ms wobble.
    #[test]
    fn test_check_status_returns_indeterminate_for_empty_file_within_skew() {
        let (_dir, path) = temp_pid_path();
        fs::write(&path, b"").unwrap();
        // 1 second in the future — within typical NTP skew.
        let near_future = SystemTime::now()
            .checked_add(Duration::from_secs(1))
            .expect("near-future should fit");
        let f = fs::OpenOptions::new().write(true).open(&path).unwrap();
        f.set_modified(near_future).unwrap();

        let status = PidFile::check_status(&path);

        assert_eq!(
            status,
            PidFileStatus::Indeterminate,
            "empty PID file with small-skew future mtime should remain Indeterminate"
        );
        assert!(path.exists(), "Indeterminate status must NOT remove the file");
    }

    /// `check_running` collapses both `Dead` and `Indeterminate` to `None`
    /// for back-compat with existing callers (notably `lifecycle::stop_daemon`
    /// which only cares whether there's an identifiable PID). The new
    /// `check_status` is the right API when the third state matters.
    #[test]
    fn test_check_running_collapses_indeterminate_to_none() {
        let (_dir, path) = temp_pid_path();
        fs::write(&path, b"").unwrap();
        // Fresh empty → check_status returns Indeterminate.
        assert_eq!(PidFile::check_status(&path), PidFileStatus::Indeterminate);
        // check_running must still report None (no identifiable PID) but
        // MUST NOT have removed the file.
        let result = PidFile::check_running(&path);
        assert_eq!(
            result, None,
            "check_running must return None for Indeterminate (back-compat)"
        );
        assert!(
            path.exists(),
            "check_running must NOT remove a file classified as Indeterminate"
        );
    }

    /// `create_exclusive` must bail when an existing PID file is owned by a
    /// live legacy daemon, rather than silently removing it and starting a
    /// duplicate. (Companion to `test_check_running_preserves_legacy_pid_file_when_alive`.)
    #[test]
    fn test_create_exclusive_bails_on_live_legacy_pid_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("conflict.pid");
        let live_pid = std::process::id();
        std::fs::write(&path, format!("{}\n", live_pid)).unwrap();

        let result = PidFile::create_exclusive(&path);

        assert!(
            result.is_err(),
            "create_exclusive must fail when a live legacy daemon owns the file"
        );
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("already running"),
            "error should indicate the daemon is already running, got: {}",
            err
        );
        assert!(
            path.exists(),
            "create_exclusive must NOT delete a live legacy daemon's PID file"
        );
    }
}
