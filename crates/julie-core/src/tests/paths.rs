//! Tests for `DaemonPaths` embedding-host path helpers (Phase 3b).

use crate::paths::DaemonPaths;
use std::path::PathBuf;

fn fixed_home() -> DaemonPaths {
    DaemonPaths::with_home(PathBuf::from("/tmp/julie-home-3b"))
}

#[cfg(unix)]
#[test]
fn embedding_host_socket_composes_under_home() {
    let paths = fixed_home();
    assert_eq!(
        paths.embedding_host_socket(),
        PathBuf::from("/tmp/julie-home-3b/embedding-host.sock")
    );
}

#[test]
fn embedding_host_lock_composes_under_home() {
    let paths = fixed_home();
    assert_eq!(
        paths.embedding_host_lock(),
        PathBuf::from("/tmp/julie-home-3b/embedding-host.lock")
    );
}

#[cfg(windows)]
#[test]
fn embedding_host_pipe_name_is_deterministic_and_prefixed() {
    let paths = fixed_home();
    let a = paths.embedding_host_pipe_name();
    let b = paths.embedding_host_pipe_name();
    // Deterministic for a fixed JULIE_HOME.
    assert_eq!(a, b);
    // Local named-pipe namespace prefix.
    assert!(
        a.starts_with(r"\\.\pipe\julie-embedding-host-"),
        "unexpected pipe name: {a}"
    );
    // Shares the same FNV-1a hash suffix as the daemon shutdown event.
    let shutdown = paths.daemon_shutdown_event();
    let hash_suffix = shutdown.rsplit('-').next().unwrap();
    assert!(
        a.ends_with(hash_suffix),
        "pipe name {a} should reuse the daemon hash suffix {hash_suffix}"
    );
}
