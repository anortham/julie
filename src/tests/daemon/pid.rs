//! Tests for PID file lifecycle management (daemon::pid module).

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

    #[test]
    fn test_create_pid_file_writes_current_pid() {
        let (_dir, path) = temp_pid_path();
        let pid_file = PidFile::create(&path).unwrap();

        // Read back the file and verify it contains current PID
        let contents = fs::read_to_string(&path).unwrap();
        let written_pid: u32 = contents.trim().parse().unwrap();
        assert_eq!(written_pid, std::process::id());

        pid_file.cleanup().unwrap();
    }

    #[test]
    fn test_read_pid_from_existing_file() {
        let (_dir, path) = temp_pid_path();
        fs::write(&path, "12345").unwrap();

        let pid = PidFile::read_pid(&path);
        assert_eq!(pid, Some(12345));
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

        // Write a PID that definitely doesn't exist
        fs::write(&path, "99999999").unwrap();
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
    fn test_check_running_returns_pid_for_live_process() {
        let (_dir, path) = temp_pid_path();

        // Write our own PID (which is definitely alive)
        let our_pid = std::process::id();
        fs::write(&path, our_pid.to_string()).unwrap();

        let result = PidFile::check_running(&path);
        assert_eq!(result, Some(our_pid), "Live process PID should be returned");
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
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents.trim().parse::<u32>().unwrap(), std::process::id());
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

        // Write a stale PID (process definitely dead)
        std::fs::write(&path, "99999999").unwrap();

        // create_exclusive should detect the dead process, remove stale file, succeed
        let result = PidFile::create_exclusive(&path);
        assert!(
            result.is_ok(),
            "create_exclusive should succeed with stale PID: {:?}",
            result.err()
        );

        let pid_str = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            pid_str.trim().parse::<u32>().unwrap(),
            std::process::id(),
            "PID file should contain current process PID after overwriting stale one"
        );

        result.unwrap().cleanup().unwrap();
    }
}
