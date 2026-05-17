//! Direct tests for `DaemonApp` — the embeddable daemon surface introduced in
//! Task A1.6 of the daemon split + reranker plan.
//!
//! These tests construct a `DaemonApp` directly (without going through the
//! `run_daemon` wrapper or spawning a subprocess) so future in-process test
//! fixtures (B.3) can build on the same surface.

use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

use crate::adapter::launcher::{DaemonLauncher, DaemonReadiness};
use crate::daemon::discovery::{AcquireError, DaemonLockGuard, DiscoveryFile, DiscoveryState};
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::mcp_session::{DaemonMcpSession, DaemonSessionDependencies};
use crate::daemon::session::SessionTracker;
use crate::daemon::singleton::SingletonLock;
use crate::daemon::transport::{TransportEndpoint, TransportProbe};
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::daemon::{DaemonApp, DaemonConfig, DaemonRuntimeContext};
use crate::paths::DaemonPaths;
use crate::workspace::mutation_gate::Registry;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_daemon_app_uses_new_daemon_lock_without_legacy_pid_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let paths = DaemonPaths::with_home(tmp.path().to_path_buf());
    paths.ensure_dirs().expect("ensure_dirs");

    let config = DaemonConfig {
        paths: paths.clone(),
        port: 0,
        no_dashboard: true,
        runtime: DaemonRuntimeContext::default(),
    };

    let _app = DaemonApp::new(config).expect("DaemonApp::new");

    assert!(
        matches!(
            DaemonLockGuard::try_acquire(&paths.daemon_lock()),
            Err(AcquireError::AlreadyHeld(_))
        ),
        "DaemonApp must hold the new daemon.lock singleton"
    );
    assert!(
        !paths.daemon_pid().exists(),
        "new daemon lifecycle must not write legacy daemon.pid"
    );
    assert!(
        !paths.daemon_singleton_lock().exists(),
        "new daemon lifecycle must not write legacy daemon.singleton.lock"
    );

    let _legacy_lock = SingletonLock::try_acquire(&paths.daemon_singleton_lock())
        .expect("legacy singleton lock must remain available for migration probes");
}

/// Spin up a `DaemonApp` on a caller-provided listener, hit the MCP readiness
/// route, then shut it down cleanly via `DaemonHandle::shutdown`.
///
/// Invariants proven:
/// - `DaemonApp::new` + `serve` produces a reachable HTTP server bound to the
///   listener we passed in.
/// - The MCP HTTP transport publishes its discovery + bearer token files so a
///   real client could talk to it.
/// - `DaemonHandle::shutdown` terminates the serve task and cleans up artifacts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_daemon_app_serve_and_shutdown() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let paths = DaemonPaths::with_home(tmp.path().to_path_buf());
    paths.ensure_dirs().expect("ensure_dirs");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mcp listener");
    let local_addr = listener.local_addr().expect("listener local_addr");

    let config = DaemonConfig {
        paths: paths.clone(),
        port: local_addr.port(),
        no_dashboard: true,
        runtime: DaemonRuntimeContext::default(),
    };

    let app = DaemonApp::new(config).expect("DaemonApp::new");
    let handle = app.serve(listener).await.expect("DaemonApp::serve");

    // Bound address must match the listener we passed in — `serve` does not
    // do a hidden rebind.
    assert_eq!(
        handle.local_addr(),
        local_addr,
        "handle.local_addr() must match the listener provided to serve()"
    );

    match DiscoveryFile::read_and_validate(&paths.discovery_file()) {
        DiscoveryState::Live(record) => {
            assert_eq!(
                record.host,
                local_addr.ip().to_string(),
                "discovery.json must publish the listener host, not the machine hostname"
            );
        }
        other => panic!("expected live discovery.json after serve, got {other:?}"),
    }
    let launcher = DaemonLauncher::new(paths.clone());
    assert_eq!(
        launcher.daemon_readiness(),
        DaemonReadiness::Ready,
        "adapter launcher must be able to probe the discovery.json endpoint"
    );

    // Probe MCP readiness via the discovery file the daemon wrote. This proves
    // the listener is reachable AND that the bearer token / discovery wiring
    // matches what a real client would observe.
    let discovery_path = paths.daemon_mcp_transport();
    let endpoint = TransportEndpoint::read_discovery(&discovery_path)
        .expect("read mcp discovery published by daemon");

    let endpoint_for_probe = endpoint.clone();
    let probe_result = tokio::task::spawn_blocking(move || endpoint_for_probe.probe_readiness())
        .await
        .expect("readiness probe task join");
    assert_eq!(
        probe_result,
        TransportProbe::Ready,
        "mcp transport at {} must answer the readiness route after DaemonApp::serve returns",
        endpoint.mcp_url().unwrap_or_default()
    );

    // Shut down cleanly. Bounded timeout: shutdown should complete promptly
    // since there are no active sessions to drain.
    tokio::time::timeout(Duration::from_secs(10), handle.shutdown())
        .await
        .expect("shutdown did not complete within 10s")
        .expect("shutdown returned an error");

    // After shutdown, both discovery formats must be gone. The MCP transport
    // owns daemon-mcp-transport.json; DaemonHandle owns discovery.json.
    assert!(
        !discovery_path.exists(),
        "mcp discovery file should be removed during shutdown, still present at {}",
        discovery_path.display()
    );
    assert!(
        !paths.discovery_file().exists(),
        "discovery.json should be removed during shutdown, still present at {}",
        paths.discovery_file().display()
    );
}

/// Two `DaemonRuntimeContext::for_test()` instances must not share gate locks.
///
/// Invariant proved: isolated runtime contexts use independent `Registry`
/// instances, so acquiring a gate in one does not block acquisition in the
/// other for the same workspace_id.
#[tokio::test]
async fn test_for_test_registries_are_isolated() {
    let a = DaemonRuntimeContext::for_test();
    let b = DaemonRuntimeContext::for_test();
    let _ga = a.mutation_gate_registry.acquire("ws").await;
    let gb = tokio::time::timeout(
        std::time::Duration::from_millis(50),
        b.mutation_gate_registry.acquire("ws"),
    )
    .await
    .expect("isolated runtime contexts must not share gate locks");
    drop(gb);
}

/// A daemon session handler must use the registry from its daemon runtime.
///
/// Invariant proved: holding the process-global gate for the same workspace
/// must not block a handler created from an isolated daemon runtime. If this
/// blocks, the runtime registry is decorative and daemon writer paths still
/// route through global state.
#[tokio::test]
async fn test_daemon_session_handler_uses_runtime_mutation_gate_registry() {
    let runtime = DaemonRuntimeContext::for_test();
    let workspace_root = tempfile::tempdir().expect("workspace tempdir");
    std::fs::write(workspace_root.path().join("lib.rs"), "fn indexed() {}\n")
        .expect("write fixture source");
    let indexes_dir = tempfile::tempdir().expect("indexes tempdir");
    let workspace_pool = Arc::new(WorkspacePool::new_isolated(
        indexes_dir.path().to_path_buf(),
        None,
    ));
    let watcher_pool = Arc::new(WatcherPool::new_with_mutation_gate_registry(
        Duration::from_secs(300),
        Arc::clone(&runtime.mutation_gate_registry),
    ));
    let dependencies = Arc::new(DaemonSessionDependencies::new(
        Arc::clone(&workspace_pool),
        None,
        Arc::new(EmbeddingService::initializing()),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        None,
        Some(watcher_pool),
        Arc::new(SessionTracker::new()),
        Arc::clone(&runtime.mutation_gate_registry),
    ));
    let startup_hint = WorkspaceStartupHint {
        path: workspace_root.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    let workspace_id =
        generate_workspace_id(&workspace_root.path().to_string_lossy()).expect("workspace id");

    let session = DaemonMcpSession::start(
        dependencies,
        "runtime-gate-test",
        startup_hint,
        None,
        "test",
    )
    .await
    .expect("start daemon session");
    let handler = session.handler();

    let _global_guard = Registry::global().acquire(&workspace_id).await;
    let runtime_guard = tokio::time::timeout(
        Duration::from_millis(100),
        handler.acquire_mutation_gate(&workspace_id),
    )
    .await
    .expect("handler must use the daemon runtime registry, not the global registry");
    drop(runtime_guard);

    session.finish().await;
}

/// `install_tracing` must be idempotent.
///
/// Invariant proved (B.2 acceptance criterion): calling `install_tracing`
/// twice in the same process succeeds without panicking. The first call
/// installs the subscriber; the second call detects via the internal
/// `OnceLock` (or `try_init`'s error path) that one is already installed and
/// returns `Ok(())` without touching it. Without this, the InProcessDaemon
/// test fixture (B.3) — which spins many daemons in one process — would
/// panic on the second daemon's tracing init.
#[test]
fn test_install_tracing_is_idempotent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let paths = DaemonPaths::with_home(tmp.path().to_path_buf());
    paths.ensure_dirs().expect("ensure_dirs");

    let ctx = DaemonRuntimeContext::default();

    // First install: may either succeed in installing or no-op (if another
    // test already installed in this process). Either way it must not panic.
    ctx.install_tracing(&paths)
        .expect("first install_tracing must not error");

    // Second install in the same process: MUST be a clean no-op. This is the
    // load-bearing assertion — the old `.init()`-based code panicked here.
    ctx.install_tracing(&paths)
        .expect("second install_tracing must not panic");

    // A third call from a different runtime context (mimicking what would
    // happen when InProcessDaemon spins a fresh runtime per test) must also
    // be a no-op.
    let ctx_b = DaemonRuntimeContext::for_test();
    ctx_b
        .install_tracing(&paths)
        .expect("install_tracing on a second runtime context must not panic");
}
