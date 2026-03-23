use crate::daemon::lifecycle::{DaemonStatus, check_status, stop_daemon};
use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;

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
