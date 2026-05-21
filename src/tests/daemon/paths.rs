use crate::paths::DaemonPaths;
use serial_test::serial;
use std::path::PathBuf;

/// Helper that sets the env var and returns a guard that restores the previous
/// value on drop. Mirrors the pattern in `src/tests/daemon/drain_timeout.rs`.
fn with_env(key: &str, value: &str) -> EnvGuard {
    let previous = std::env::var(key).ok();
    // SAFETY: single-threaded by serial attribute; no other threads read this var.
    unsafe { std::env::set_var(key, value) };
    EnvGuard {
        key: key.to_owned(),
        previous,
    }
}

fn without_env(key: &str) -> EnvGuard {
    let previous = std::env::var(key).ok();
    // SAFETY: single-threaded by serial attribute; no other threads read this var.
    unsafe { std::env::remove_var(key) };
    EnvGuard {
        key: key.to_owned(),
        previous,
    }
}

struct EnvGuard {
    key: String,
    previous: Option<String>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => unsafe { std::env::set_var(&self.key, v) },
            None => unsafe { std::env::remove_var(&self.key) },
        }
    }
}

const JULIE_HOME_ENV: &str = "JULIE_HOME";

#[test]
#[serial(julie_home_env)]
fn test_julie_home_uses_home_dir() {
    let _guard = without_env(JULIE_HOME_ENV);
    let paths = DaemonPaths::new();
    let home = dirs::home_dir().unwrap();
    assert_eq!(paths.julie_home(), home.join(".julie"));
}

#[test]
#[serial(julie_home_env)]
fn test_julie_home_env_override() {
    let tmp = tempfile::tempdir().unwrap();
    let override_home = tmp.path().join("external-julie-home");
    let _guard = with_env(JULIE_HOME_ENV, override_home.to_str().unwrap());

    let paths = DaemonPaths::try_new().expect("try_new should succeed when JULIE_HOME is set");
    assert_eq!(paths.julie_home(), override_home);
    assert_eq!(paths.indexes_dir(), override_home.join("indexes"));
    assert_eq!(paths.daemon_db(), override_home.join("daemon.db"));
}

#[test]
#[serial(julie_home_env)]
fn test_julie_home_env_empty_is_rejected() {
    let _guard = with_env(JULIE_HOME_ENV, "");

    match DaemonPaths::try_new() {
        Ok(_) => panic!("empty JULIE_HOME must be rejected"),
        Err(err) => {
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
            assert!(
                err.to_string().contains("JULIE_HOME"),
                "error message should mention JULIE_HOME, got: {}",
                err
            );
        }
    }
}

#[test]
fn test_is_julie_home_matches_canonicalized_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let paths = DaemonPaths::with_home(home.clone());
    assert!(paths.is_julie_home(&home));
}

#[test]
fn test_is_julie_home_rejects_unrelated_path() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let other = tmp.path().join("other");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let paths = DaemonPaths::with_home(home);
    assert!(!paths.is_julie_home(&other));
}

#[cfg(target_os = "macos")]
#[test]
fn test_is_julie_home_case_insensitive_on_macos() {
    let tmp = tempfile::tempdir().unwrap();
    let upper = tmp.path().join("Home");
    std::fs::create_dir_all(&upper).unwrap();
    let lower = tmp.path().join("home");
    let paths = DaemonPaths::with_home(upper);
    assert!(
        paths.is_julie_home(&lower),
        "macOS comparison must be case-insensitive"
    );
}

#[test]
#[serial(julie_home_env)]
fn test_indexes_dir() {
    let _guard = without_env(JULIE_HOME_ENV);
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("indexes");
    assert_eq!(paths.indexes_dir(), expected);
}

#[test]
#[serial(julie_home_env)]
fn test_workspace_index_path() {
    let _guard = without_env(JULIE_HOME_ENV);
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir()
        .unwrap()
        .join(".julie")
        .join("indexes")
        .join("myproject_abc12345");
    assert_eq!(paths.workspace_index_dir("myproject_abc12345"), expected);
}

#[test]
#[serial(julie_home_env)]
fn test_workspace_db_path() {
    let _guard = without_env(JULIE_HOME_ENV);
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
#[serial(julie_home_env)]
fn test_workspace_tantivy_path() {
    let _guard = without_env(JULIE_HOME_ENV);
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
#[serial(julie_home_env)]
fn test_daemon_pid_path() {
    let _guard = without_env(JULIE_HOME_ENV);
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("daemon.pid");
    assert_eq!(paths.daemon_pid(), expected);
}

#[test]
#[serial(julie_home_env)]
fn test_daemon_lock_path() {
    let _guard = without_env(JULIE_HOME_ENV);
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("daemon.lock");
    assert_eq!(paths.daemon_lock(), expected);
}

#[test]
#[serial(julie_home_env)]
fn test_daemon_log_path() {
    let _guard = without_env(JULIE_HOME_ENV);
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir().unwrap().join(".julie").join("daemon.log");
    assert_eq!(paths.daemon_log(), expected);
}

#[test]
#[serial(julie_home_env)]
fn test_project_log_dir() {
    // project_log_dir doesn't depend on julie_home, but guard for consistency
    // so the test cannot panic on an invalid JULIE_HOME-influenced path.
    let _guard = without_env(JULIE_HOME_ENV);
    let project = PathBuf::from("/Users/murphy/source/julie");
    let paths = DaemonPaths::new();
    assert_eq!(
        paths.project_log_dir(&project),
        project.join(".julie").join("logs")
    );
}

#[test]
#[serial(julie_home_env)]
fn test_migration_state_path() {
    let _guard = without_env(JULIE_HOME_ENV);
    let paths = DaemonPaths::new();
    let expected = dirs::home_dir()
        .unwrap()
        .join(".julie")
        .join("migration.json");
    assert_eq!(paths.migration_state(), expected);
}

#[test]
fn test_daemon_mcp_transport_paths_are_distinct_from_dashboard_port() {
    let paths = DaemonPaths::with_home(PathBuf::from("/tmp/test-julie"));
    assert_eq!(
        paths.daemon_mcp_transport(),
        PathBuf::from("/tmp/test-julie/daemon-mcp-transport.json")
    );
    assert_eq!(
        paths.daemon_mcp_token(),
        PathBuf::from("/tmp/test-julie/daemon-mcp.token")
    );
    assert_ne!(
        paths.daemon_mcp_transport(),
        paths.daemon_port(),
        "MCP Streamable HTTP discovery must not reuse the dashboard port file"
    );
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
#[serial(julie_home_env)]
fn test_default_impl() {
    let _guard = without_env(JULIE_HOME_ENV);
    // Default should behave the same as new()
    let default_paths = DaemonPaths::default();
    let new_paths = DaemonPaths::new();
    assert_eq!(default_paths.julie_home(), new_paths.julie_home());
}

#[cfg(windows)]
#[test]
fn test_daemon_shutdown_event_is_scoped_to_julie_home() {
    let paths_a = DaemonPaths::with_home(PathBuf::from(r"C:\Users\test-a\.julie"));
    let paths_b = DaemonPaths::with_home(PathBuf::from(r"C:\Users\test-b\.julie"));
    let paths_a_again = DaemonPaths::with_home(PathBuf::from(r"C:\Users\test-a\.julie"));

    assert_ne!(
        paths_a.daemon_shutdown_event(),
        paths_b.daemon_shutdown_event(),
        "Different JULIE_HOME values must have different shutdown events"
    );
    assert_eq!(
        paths_a.daemon_shutdown_event(),
        paths_a_again.daemon_shutdown_event(),
        "Shutdown event names must be deterministic for one JULIE_HOME"
    );
}
