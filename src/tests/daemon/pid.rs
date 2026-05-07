//! Tests for PID file lifecycle management (daemon::pid module).

#[cfg(test)]
mod tests {
    use crate::daemon::pid::{PidFile, PidFileContents};
    use std::fs;
    use tempfile::TempDir;

    fn temp_pid_path() -> (TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("julie.pid");
        (dir, path)
    }

    /// Write a valid three-field PID file with a dead (stale) process PID.
    fn write_stale_pid_file(path: &std::path::Path) {
        // PID 99999999 does not exist; creation_time 1 is a dead giveaway sentinel.
        fs::write(path, "99999999 1 0\n").unwrap();
    }

    #[test]
    fn test_create_pid_file_writes_current_pid() {
        let (_dir, path) = temp_pid_path();
        let pid_file = PidFile::create(&path).unwrap();

        // The file now uses the three-field format — use read_pid to extract the PID.
        let written_pid = PidFile::read_pid(&path).expect("read_pid must succeed");
        assert_eq!(written_pid, std::process::id());

        pid_file.cleanup().unwrap();
    }

    #[test]
    fn test_read_pid_from_existing_file() {
        let (_dir, path) = temp_pid_path();
        // The new format requires three fields; a single integer is legacy and returns None.
        // Write a valid three-field file to confirm read_pid works.
        fs::write(&path, "12345 1000000 0\n").unwrap();

        let pid = PidFile::read_pid(&path);
        assert_eq!(pid, Some(12345));
    }

    #[test]
    fn test_read_pid_returns_none_for_legacy_single_int() {
        // Legacy single-integer files are treated as stale and return None.
        let (_dir, path) = temp_pid_path();
        fs::write(&path, "12345").unwrap();

        let pid = PidFile::read_pid(&path);
        assert_eq!(
            pid, None,
            "legacy single-integer PID files must return None (treated as stale)"
        );
    }

    #[test]
    fn test_read_pid_returns_none_for_missing_file() {
        let (_dir, path) = temp_pid_path();
        // Don't create the file
        let pid = PidFile::read_pid(&path);
        assert_eq!(pid, None);
    }

    #[test]
    fn test_read_pid_returns_none_for_corrupt_file() {
        let (_dir, path) = temp_pid_path();
        fs::write(&path, "not-a-number").unwrap();

        let pid = PidFile::read_pid(&path);
        assert_eq!(pid, None);
    }

    #[test]
    fn test_pid_file_contents_parse_three_fields() {
        let c = PidFileContents::parse("12345 9999999 8888888\n").unwrap();
        assert_eq!(c.pid, 12345);
        assert_eq!(c.creation_time_micros, 9999999);
        assert_eq!(c.binary_mtime_micros, 8888888);
    }

    #[test]
    fn test_pid_file_contents_parse_rejects_single_int() {
        assert!(
            PidFileContents::parse("12345").is_none(),
            "single-int legacy format must be rejected"
        );
    }

    #[test]
    fn test_is_process_alive_for_current_process() {
        // Our own process should be alive
        let alive = PidFile::is_process_alive(std::process::id());
        assert!(alive, "Current process should be reported as alive");
    }

    #[test]
    fn test_is_process_alive_for_nonexistent_pid() {
        // PID 99999999 should not exist on any reasonable system
        let alive = PidFile::is_process_alive(99_999_999);
        assert!(!alive, "Non-existent PID should be reported as dead");
    }

    #[test]
    fn test_create_pid_file_is_atomic() {
        let (_dir, path) = temp_pid_path();
        let tmp_path = path.with_extension("pid.tmp");

        let pid_file = PidFile::create(&path).unwrap();

        // The .tmp file used during atomic write should not remain
        assert!(
            !tmp_path.exists(),
            ".tmp file should not remain after create"
        );
        // But the actual PID file should exist
        assert!(path.exists(), "PID file should exist after create");

        pid_file.cleanup().unwrap();
    }

    #[test]
    fn test_cleanup_removes_pid_file() {
        let (_dir, path) = temp_pid_path();
        let pid_file = PidFile::create(&path).unwrap();
        assert!(path.exists());

        pid_file.cleanup().unwrap();
        assert!(!path.exists(), "PID file should be removed after cleanup");
    }

    #[test]
    fn test_check_running_cleans_stale_pid() {
        let (_dir, path) = temp_pid_path();

        // Write a three-field file with a PID that definitely doesn't exist.
        write_stale_pid_file(&path);
        assert!(path.exists());

        // check_running should detect the process is dead, clean up, and return None
        let result = PidFile::check_running(&path);
        assert_eq!(result, None, "Stale PID should return None");
        assert!(
            !path.exists(),
            "Stale PID file should be cleaned up automatically"
        );
    }

    #[test]
    fn test_check_running_preserves_live_legacy_format() {
        // Updated post-codex-review: a live legacy daemon's single-integer
        // PID file must NOT be deleted by a v7.7.x adapter. Otherwise an
        // upgrade where the user has v7.7.<earlier> running and a new
        // adapter starts would delete the running daemon's PID file and
        // spawn a duplicate — breaking the single-daemon invariant.
        let (_dir, path) = temp_pid_path();
        let our_pid = std::process::id();
        fs::write(&path, our_pid.to_string()).unwrap();
        assert!(path.exists());

        let result = PidFile::check_running(&path);
        assert_eq!(
            result,
            Some(our_pid),
            "Live legacy PID file must be reported as running"
        );
        assert!(
            path.exists(),
            "Live legacy PID file must NOT be deleted by check_running"
        );
    }

    #[test]
    fn test_check_running_returns_pid_for_live_process() {
        let (_dir, path) = temp_pid_path();

        // Use create_exclusive so the file is in the correct three-field format
        // with the real creation_time for this process.
        let pid_file = PidFile::create_exclusive(&path).unwrap();

        let result = PidFile::check_running(&path);
        assert_eq!(result, Some(std::process::id()), "Live process PID should be returned");

        pid_file.cleanup().unwrap();
    }

    #[test]
    fn test_check_running_returns_none_for_missing_file() {
        let (_dir, path) = temp_pid_path();
        // No file exists
        let result = PidFile::check_running(&path);
        assert_eq!(result, None);
    }

    // ── D-C2 ──────────────────────────────────────────────────────────────────
    // create_exclusive must atomically check-and-create the PID file so two
    // concurrent `julie daemon` invocations can't both believe they are first.

    #[test]
    fn test_create_exclusive_succeeds_when_no_file_exists() {
        let (_dir, path) = temp_pid_path();
        let pid_file = PidFile::create_exclusive(&path).unwrap();
        // Use read_pid (not raw parse) since the file is now three-field format.
        let pid = PidFile::read_pid(&path).expect("read_pid must succeed after create_exclusive");
        assert_eq!(pid, std::process::id());
        pid_file.cleanup().unwrap();
    }

    #[test]
    fn test_create_exclusive_detects_live_process() {
        let (_dir, path) = temp_pid_path();

        // First exclusive creation succeeds
        let _pid1 = PidFile::create_exclusive(&path).unwrap();

        // Second attempt while the first process (us) is still alive must fail
        let result = PidFile::create_exclusive(&path);
        assert!(
            result.is_err(),
            "second create_exclusive should fail while process is live"
        );
        assert!(
            format!("{:?}", result.unwrap_err()).contains("already running"),
            "error should mention 'already running'"
        );
    }

    #[test]
    fn test_create_exclusive_handles_stale_pid() {
        let (_dir, path) = temp_pid_path();

        // Write a three-field stale PID file (dead process, wrong creation_time).
        write_stale_pid_file(&path);

        // create_exclusive should detect the dead process, remove stale file, succeed.
        let result = PidFile::create_exclusive(&path);
        assert!(
            result.is_ok(),
            "create_exclusive should succeed with stale PID: {:?}",
            result.err()
        );

        // Verify the new file contains our PID via read_pid (three-field format).
        let new_pid = PidFile::read_pid(&path).expect("read_pid must succeed after overwrite");
        assert_eq!(
            new_pid,
            std::process::id(),
            "PID file should contain current process PID after overwriting stale one"
        );

        result.unwrap().cleanup().unwrap();
    }
}
