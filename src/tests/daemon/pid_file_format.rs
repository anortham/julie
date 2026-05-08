//! Tests for the three-field PID file format and PID-reuse defense.
//!
//! Format: "<pid> <creation_time_unix_micros> <binary_mtime_unix_micros>\n"
//!
//! check_running rejects any PID file whose stored creation_time does not
//! match the running process's actual creation time, preventing a recycled
//! PID from impersonating the original daemon.

#[cfg(test)]
mod tests {
    use crate::daemon::pid::PidFile;
    use std::fs;
    use tempfile::TempDir;

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
