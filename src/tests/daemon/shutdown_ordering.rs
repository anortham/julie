//! Tests verifying that `perform_shutdown_sequence` executes shutdown steps in
//! LIFO dependency order:
//!   1. HTTP transport (stops new requests)
//!   2. Embedding service
//!   3. WorkspacePool (commits Tantivy writes, releases file locks)
//!   4. WatcherPool (drops OS file-watcher handles)
//!   5. Housekeeping (port file, pid file, state file)
//!
//! Ordering is verified via a `call_log: Arc<Mutex<Vec<&'static str>>>` that
//! `perform_shutdown_sequence` writes to before each step in test mode.
//! This avoids the need for trait objects or async-closure bounds.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::http_transport::{HttpTransportConfig, HttpTransportServer};
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::daemon::{ShutdownArtifacts, perform_shutdown_sequence};
use crate::paths::DaemonPaths;

// ---- helpers ----

fn make_daemon_paths() -> (DaemonPaths, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = DaemonPaths::with_home(dir.path().join("julie-home"));
    paths.ensure_dirs().expect("ensure_dirs");
    (paths, dir)
}

async fn bind_test_transport(paths: DaemonPaths) -> HttpTransportServer {
    use rmcp::ServerHandler;

    #[derive(Clone)]
    struct NoopHandler;
    impl ServerHandler for NoopHandler {}

    HttpTransportServer::bind(
        paths,
        HttpTransportConfig {
            bind_host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            ..HttpTransportConfig::default()
        },
        || Ok(NoopHandler),
    )
    .await
    .expect("bind test transport")
}

// ---- tests ----

/// `WorkspacePool::shutdown` and `WatcherPool::shutdown` must be called AFTER
/// `http_transport.shutdown` completes. The call_log records observed order;
/// we assert that "http_transport" appears before "workspace_pool" and
/// "watcher_pool" in the log.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_shutdown_calls_pools_after_transport() {
    let (paths, _dir) = make_daemon_paths();
    let http_transport = bind_test_transport(paths.clone()).await;
    let embedding_service = Arc::new(EmbeddingService::initialize_for_test(None));
    let workspace_pool = Arc::new(WorkspacePool::new(paths.indexes_dir().to_path_buf(), None));
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));

    let call_log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

    let port_path = paths.daemon_port();
    let discovery_path = paths.discovery_file();
    let state_path = paths.daemon_state();

    let artifacts = ShutdownArtifacts {
        port_path: &port_path,
        discovery_path: &discovery_path,
        state_path: &state_path,
    };
    perform_shutdown_sequence(
        http_transport,
        embedding_service,
        workspace_pool,
        watcher_pool,
        artifacts,
        Some(Arc::clone(&call_log)),
    )
    .await;

    let log = call_log.lock().expect("call_log mutex");
    let http_pos = log
        .iter()
        .position(|s| *s == "http_transport")
        .expect("http_transport must appear in call log");
    let ws_pos = log
        .iter()
        .position(|s| *s == "workspace_pool")
        .expect("workspace_pool must appear in call log");
    let watcher_pos = log
        .iter()
        .position(|s| *s == "watcher_pool")
        .expect("watcher_pool must appear in call log");

    assert!(
        http_pos < ws_pos,
        "http_transport ({http_pos}) must complete before workspace_pool ({ws_pos}); log={log:?}"
    );
    assert!(
        http_pos < watcher_pos,
        "http_transport ({http_pos}) must complete before watcher_pool ({watcher_pos}); log={log:?}"
    );
}

/// `WorkspacePool::shutdown` must be called before `WatcherPool::shutdown`.
/// Tantivy file locks must be released before OS file-watcher handles are
/// dropped, so a fast-starting new daemon does not race with lingering locks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_shutdown_calls_workspace_pool_before_watcher_pool() {
    let (paths, _dir) = make_daemon_paths();
    let http_transport = bind_test_transport(paths.clone()).await;
    let embedding_service = Arc::new(EmbeddingService::initialize_for_test(None));
    let workspace_pool = Arc::new(WorkspacePool::new(paths.indexes_dir().to_path_buf(), None));
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));

    let call_log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));

    let port_path = paths.daemon_port();
    let discovery_path = paths.discovery_file();
    let state_path = paths.daemon_state();

    let artifacts = ShutdownArtifacts {
        port_path: &port_path,
        discovery_path: &discovery_path,
        state_path: &state_path,
    };
    perform_shutdown_sequence(
        http_transport,
        embedding_service,
        workspace_pool,
        watcher_pool,
        artifacts,
        Some(Arc::clone(&call_log)),
    )
    .await;

    let log = call_log.lock().expect("call_log mutex");
    let ws_pos = log
        .iter()
        .position(|s| *s == "workspace_pool")
        .expect("workspace_pool must appear in call log");
    let watcher_pos = log
        .iter()
        .position(|s| *s == "watcher_pool")
        .expect("watcher_pool must appear in call log");

    assert!(
        ws_pos < watcher_pos,
        "workspace_pool ({ws_pos}) must shut down before watcher_pool ({watcher_pos}); log={log:?}"
    );
}
