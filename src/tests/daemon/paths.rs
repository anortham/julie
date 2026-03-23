use crate::paths::DaemonPaths;
use std::path::PathBuf;

#[test]
fn test_julie_home_uses_home_dir() {
    let paths = DaemonPaths::new();
    let home = dirs::home_dir().unwrap();
    assert_eq!(paths.julie_home(), home.join(".julie"));
}

#[test]
fn test_indexes_dir() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("indexes");
    assert_eq!(paths.indexes_dir(), expected);
}

#[test]
fn test_workspace_index_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir()
        .unwrap()
        .join(".julie")
        .join("indexes")
        .join("myproject_abc12345");
    assert_eq!(paths.workspace_index_dir("myproject_abc12345"), expected);
}

#[test]
fn test_workspace_db_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir()
        .unwrap()
        .join(".julie")
        .join("indexes")
        .join("myproject_abc12345")
        .join("db")
        .join("symbols.db");
    assert_eq!(paths.workspace_db_path("myproject_abc12345"), expected);
}

#[test]
fn test_workspace_tantivy_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir()
        .unwrap()
        .join(".julie")
        .join("indexes")
        .join("myproject_abc12345")
        .join("tantivy");
    assert_eq!(paths.workspace_tantivy_path("myproject_abc12345"), expected);
}

#[test]
fn test_daemon_socket_path() {
    let paths = DaemonPaths::new();
    let julie_home = dirs::home_dir().unwrap().join(".julie");
    #[cfg(unix)]
    assert_eq!(paths.daemon_socket(), julie_home.join("daemon.sock"));
    #[cfg(windows)]
    assert_eq!(paths.daemon_pipe_name(), r"\\.\pipe\julie-daemon");
}

#[test]
fn test_daemon_pid_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("daemon.pid");
    assert_eq!(paths.daemon_pid(), expected);
}

#[test]
fn test_daemon_lock_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("daemon.lock");
    assert_eq!(paths.daemon_lock(), expected);
}

#[test]
fn test_daemon_log_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("daemon.log");
    assert_eq!(paths.daemon_log(), expected);
}

#[test]
fn test_project_log_dir() {
    let project = PathBuf::from("/Users/murphy/source/julie");
    let paths = DaemonPaths::new();
    assert_eq!(
        paths.project_log_dir(&project),
        project.join(".julie").join("logs")
    );
}

#[test]
fn test_migration_state_path() {
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir()
        .unwrap()
        .join(".julie")
        .join("migration.json");
    assert_eq!(paths.migration_state(), expected);
}

#[test]
fn test_custom_julie_home() {
    let paths = DaemonPaths::with_home(PathBuf::from("/tmp/test-julie"));
    assert_eq!(paths.julie_home(), PathBuf::from("/tmp/test-julie"));
    assert_eq!(
        paths.indexes_dir(),
        PathBuf::from("/tmp/test-julie/indexes")
    );
}

#[test]
fn test_ensure_dirs_creates_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(tmp.path().join("julie-test-home"));
    // Directory should not exist yet
    assert!(!paths.julie_home().exists());
    // ensure_dirs should create both julie_home and indexes
    paths.ensure_dirs().unwrap();
    assert!(paths.julie_home().exists());
    assert!(paths.indexes_dir().exists());
}

#[test]
fn test_default_impl() {
    // Default should behave the same as new()
    let default_paths = DaemonPaths::default();
    let new_paths = DaemonPaths::new();
    assert_eq!(default_paths.julie_home(), new_paths.julie_home());
}
