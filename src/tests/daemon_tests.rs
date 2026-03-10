//! Tests for daemon lifecycle: PID file management, process checking, start/stop/status.

use std::fs;
use std::path::PathBuf;

use crate::daemon::{
    DaemonInfo, julie_home, is_daemon_running, lock_and_write_pid_file, pid_file_path,
    remove_pid_file, write_pid_file,
};

// ============================================================================
// julie_home() TESTS
// ============================================================================

#[test]
fn test_julie_home_returns_dot_julie_in_home_dir() {
    let home = julie_home().unwrap();
    // Both Unix and Windows use ~/.julie
    assert!(
        home.ends_with(".julie"),
        "Expected path ending with .julie, got {:?}",
        home
    );
}

#[test]
fn test_julie_home_is_absolute() {
    let home = julie_home().unwrap();
    assert!(home.is_absolute(), "julie_home() should return an absolute path, got {:?}", home);
}

// ============================================================================
// PID FILE PATH TESTS
// ============================================================================

#[test]
fn test_pid_file_path_is_under_julie_home() {
    let pid_path = pid_file_path().unwrap();
    let home = julie_home().unwrap();
    assert!(
        pid_path.starts_with(&home),
        "PID file path {:?} should be under julie_home {:?}",
        pid_path,
        home
    );
    assert!(
        pid_path.ends_with("daemon.pid"),
        "PID file should be named daemon.pid, got {:?}",
        pid_path
    );
}

// ============================================================================
// PID FILE WRITE/READ/REMOVE TESTS (use tempdir to avoid polluting real ~/.julie)
// ============================================================================

/// Helper: create a temporary PID file path for testing
fn temp_pid_file() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("daemon.pid");
    (dir, pid_path)
}

#[test]
fn test_write_pid_file_creates_valid_toml() {
    let (_dir, pid_path) = temp_pid_file();

    write_pid_file(&pid_path, 12345, 7890).unwrap();

    let content = fs::read_to_string(&pid_path).unwrap();
    let parsed: DaemonInfo = toml::from_str(&content).unwrap();
    assert_eq!(parsed.pid, 12345);
    assert_eq!(parsed.port, 7890);
}

#[test]
fn test_write_pid_file_overwrites_existing() {
    let (_dir, pid_path) = temp_pid_file();

    write_pid_file(&pid_path, 111, 1111).unwrap();
    write_pid_file(&pid_path, 222, 2222).unwrap();

    let content = fs::read_to_string(&pid_path).unwrap();
    let parsed: DaemonInfo = toml::from_str(&content).unwrap();
    assert_eq!(parsed.pid, 222);
    assert_eq!(parsed.port, 2222);
}

#[test]
fn test_remove_pid_file_removes_file() {
    let (_dir, pid_path) = temp_pid_file();

    write_pid_file(&pid_path, 12345, 7890).unwrap();
    assert!(pid_path.exists());

    remove_pid_file(&pid_path).unwrap();
    assert!(!pid_path.exists());
}

#[test]
fn test_remove_pid_file_noop_if_missing() {
    let (_dir, pid_path) = temp_pid_file();
    // Should not error when file doesn't exist
    remove_pid_file(&pid_path).unwrap();
}

#[test]
fn test_read_daemon_info_from_pid_file() {
    let (_dir, pid_path) = temp_pid_file();

    write_pid_file(&pid_path, 42, 8080).unwrap();

    let info = crate::daemon::read_pid_file(&pid_path).unwrap();
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.pid, 42);
    assert_eq!(info.port, 8080);
}

#[test]
fn test_read_daemon_info_returns_none_if_missing() {
    let (_dir, pid_path) = temp_pid_file();
    let info = crate::daemon::read_pid_file(&pid_path).unwrap();
    assert!(info.is_none());
}

#[test]
fn test_read_daemon_info_returns_error_on_corrupt_file() {
    let (_dir, pid_path) = temp_pid_file();
    fs::write(&pid_path, "this is not valid toml {{{{").unwrap();
    let result = crate::daemon::read_pid_file(&pid_path);
    assert!(result.is_err(), "Corrupt PID file should return an error");
}

// ============================================================================
// PROCESS CHECKING TESTS
// ============================================================================

#[test]
fn test_is_daemon_running_returns_none_when_no_pid_file() {
    let (_dir, pid_path) = temp_pid_file();
    let result = is_daemon_running(&pid_path);
    assert!(result.is_none(), "No PID file should mean no daemon running");
}

#[test]
fn test_is_daemon_running_returns_none_for_dead_process() {
    let (_dir, pid_path) = temp_pid_file();
    // PID 99999999 almost certainly doesn't exist
    write_pid_file(&pid_path, 99999999, 7890).unwrap();
    let result = is_daemon_running(&pid_path);
    assert!(result.is_none(), "Dead process should return None");
}

#[test]
fn test_is_daemon_running_returns_info_for_current_process() {
    let (_dir, pid_path) = temp_pid_file();
    // Our own PID definitely exists
    let our_pid = std::process::id();
    write_pid_file(&pid_path, our_pid, 7890).unwrap();
    let result = is_daemon_running(&pid_path);
    assert!(
        result.is_some(),
        "Current process should be detected as running"
    );
    let info = result.unwrap();
    assert_eq!(info.pid, our_pid);
    assert_eq!(info.port, 7890);
}

#[test]
fn test_is_daemon_running_cleans_up_stale_pid_file() {
    let (_dir, pid_path) = temp_pid_file();
    // Write a PID file for a dead process
    write_pid_file(&pid_path, 99999999, 7890).unwrap();
    assert!(pid_path.exists());

    // is_daemon_running should clean up the stale file
    let result = is_daemon_running(&pid_path);
    assert!(result.is_none());
    assert!(!pid_path.exists(), "Stale PID file should be cleaned up");
}

// ============================================================================
// DAEMON INFO SERIALIZATION TESTS
// ============================================================================

#[test]
fn test_daemon_info_toml_roundtrip() {
    let info = DaemonInfo {
        pid: 12345,
        port: 7890,
    };
    let serialized = toml::to_string(&info).unwrap();
    let deserialized: DaemonInfo = toml::from_str(&serialized).unwrap();
    assert_eq!(deserialized.pid, 12345);
    assert_eq!(deserialized.port, 7890);
}

#[test]
fn test_daemon_info_toml_format_is_readable() {
    let info = DaemonInfo {
        pid: 12345,
        port: 7890,
    };
    let serialized = toml::to_string(&info).unwrap();
    // Should be human-readable key = value format
    assert!(serialized.contains("pid = 12345"), "TOML should contain 'pid = 12345', got: {}", serialized);
    assert!(serialized.contains("port = 7890"), "TOML should contain 'port = 7890', got: {}", serialized);
}

// ============================================================================
// PID FILE LOCKING TESTS
// ============================================================================

#[test]
fn test_lock_and_write_pid_file_creates_valid_toml() {
    let (_dir, pid_path) = temp_pid_file();

    let _lock = lock_and_write_pid_file(&pid_path, 12345, 7890).unwrap();

    let content = fs::read_to_string(&pid_path).unwrap();
    let parsed: DaemonInfo = toml::from_str(&content).unwrap();
    assert_eq!(parsed.pid, 12345);
    assert_eq!(parsed.port, 7890);
}

#[test]
fn test_lock_and_write_pid_file_prevents_second_lock() {
    let (_dir, pid_path) = temp_pid_file();

    // First lock should succeed
    let _lock = lock_and_write_pid_file(&pid_path, 12345, 7890).unwrap();

    // Second lock on the same file should fail
    let result = lock_and_write_pid_file(&pid_path, 67890, 8080);
    assert!(
        result.is_err(),
        "Second lock_and_write_pid_file should fail while first lock is held"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("already running"),
        "Error should mention 'already running', got: {}",
        err_msg
    );
}

#[test]
fn test_lock_released_on_drop() {
    let (_dir, pid_path) = temp_pid_file();

    // Acquire and drop the lock
    {
        let _lock = lock_and_write_pid_file(&pid_path, 12345, 7890).unwrap();
        // _lock dropped here
    }

    // Should be able to acquire the lock again after drop
    let _lock = lock_and_write_pid_file(&pid_path, 67890, 8080).unwrap();

    // Verify the new content was written
    let content = fs::read_to_string(&pid_path).unwrap();
    let parsed: DaemonInfo = toml::from_str(&content).unwrap();
    assert_eq!(parsed.pid, 67890);
    assert_eq!(parsed.port, 8080);
}

#[test]
fn test_locked_pid_file_can_still_be_read() {
    let (_dir, pid_path) = temp_pid_file();

    let _lock = lock_and_write_pid_file(&pid_path, 12345, 7890).unwrap();

    // Other processes can still read the locked file (flock doesn't prevent reads)
    let info = crate::daemon::read_pid_file(&pid_path).unwrap();
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.pid, 12345);
    assert_eq!(info.port, 7890);
}

#[test]
fn test_locked_pid_file_can_be_removed_on_unix() {
    let (_dir, pid_path) = temp_pid_file();

    let _lock = lock_and_write_pid_file(&pid_path, 12345, 7890).unwrap();

    // On Unix, removing a locked file is fine (lock is on the fd, not the path)
    #[cfg(unix)]
    {
        remove_pid_file(&pid_path).unwrap();
        assert!(!pid_path.exists());
    }
}
