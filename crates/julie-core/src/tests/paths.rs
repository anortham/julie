//! Tests for `DaemonPaths` path helpers (Phase 3b + 3c).

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

// --- Phase 3c: workspace_leader_lock ---

#[test]
fn workspace_leader_lock_is_inside_workspace_index_dir_and_distinct_from_rebuild_lock() {
    let paths = DaemonPaths::with_home(PathBuf::from("/tmp/julie-home-3c"));
    let ws = "ws123";

    let lock = paths.workspace_leader_lock(ws);
    let db = paths.workspace_db_path(ws);
    let tantivy = paths.workspace_tantivy_path(ws);
    let index_dir = paths.workspace_index_dir(ws);

    // Leader lock is directly inside the workspace index dir as leader.lock.
    assert_eq!(
        lock.parent().unwrap(),
        index_dir.as_path(),
        "leader lock must sit directly in the workspace index dir"
    );
    assert_eq!(
        lock.file_name().unwrap(),
        "leader.lock",
        "leader lock filename must be `leader.lock`"
    );

    // Ends with the canonical relative path indexes/{ws}/leader.lock.
    assert!(
        lock.ends_with("indexes/ws123/leader.lock"),
        "unexpected full path: {}",
        lock.display()
    );

    // db and tantivy share the same workspace index-dir parent.
    assert!(
        db.starts_with(&index_dir),
        "db path must be under the workspace index dir"
    );
    assert!(
        tantivy.starts_with(&index_dir),
        "tantivy path must be under the workspace index dir"
    );

    // Must NOT alias the Tantivy rebuild lock (indexes/{ws}/tantivy.julie-rebuild.lock).
    let rebuild_lock = index_dir.join("tantivy.julie-rebuild.lock");
    assert_ne!(
        lock, rebuild_lock,
        "workspace_leader_lock must not alias the Tantivy rebuild lock"
    );
}
