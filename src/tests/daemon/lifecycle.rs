use crate::daemon::lifecycle::{DaemonStatus, check_status, stop_daemon};
use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;
use std::fs;
#[cfg(unix)]
use libc;

#[test]
fn test_status_reports_not_running_when_no_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    assert_eq!(check_status(&paths), DaemonStatus::NotRunning);
}

#[test]
fn test_status_reports_running_with_valid_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    let _pid = PidFile::create(&paths.daemon_pid()).unwrap();
    match check_status(&paths) {
        DaemonStatus::Running { pid } => assert_eq!(pid, std::process::id()),
        other => panic!("Expected Running, got {:?}", other),
    }
}

#[test]
fn test_status_detects_stale_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    std::fs::create_dir_all(dir.path()).unwrap();
    // PID 99999999 should not be alive on any reasonable system
    std::fs::write(paths.daemon_pid(), "99999999\n").unwrap();
    assert_eq!(check_status(&paths), DaemonStatus::NotRunning);
}

#[test]
fn test_stop_when_not_running_is_ok() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    assert!(stop_daemon(&paths).is_ok());
}

#[test]
fn test_stop_daemon_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    let result = stop_daemon(&paths);
    assert!(result.is_ok());
}

#[test]
fn test_stop_daemon_cleans_stale_pid() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();
    // Write a bogus PID that's not alive
    fs::write(paths.daemon_pid(), "99999999").unwrap();
    fs::write(paths.daemon_state(), "ready").unwrap();

    let result = stop_daemon(&paths);
    assert!(result.is_ok());
    // Stale files should be cleaned
    assert!(!paths.daemon_pid().exists());
    assert!(!paths.daemon_state().exists());
}

#[cfg(unix)]
#[test]
fn test_stop_daemon_does_not_delete_files_while_process_alive() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    fs::create_dir_all(dir.path()).unwrap();

    // Ignore SIGTERM so the test process survives the signal
    unsafe {
        libc::signal(libc::SIGTERM, libc::SIG_IGN);
    }

    // Write current process PID (definitely alive)
    let _pid_file = PidFile::create(&paths.daemon_pid()).unwrap();
    fs::write(paths.daemon_state(), "ready").unwrap();

    // stop_daemon will try to signal, the process won't die (it's us),
    // so after the timeout it should NOT delete the files.
    let result = stop_daemon(&paths);

    // Restore default SIGTERM handler
    unsafe {
        libc::signal(libc::SIGTERM, libc::SIG_DFL);
    }

    // The function should return an error since the process didn't exit
    assert!(result.is_err(), "Should error when daemon doesn't exit");

    // PID file should still exist (not deleted under a live process)
    assert!(
        paths.daemon_pid().exists(),
        "PID file should not be deleted while process is alive"
    );
}
