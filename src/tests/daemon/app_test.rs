//! Direct tests for `DaemonApp` — the embeddable daemon surface introduced in
//! Task A1.6 of the daemon split + reranker plan.
//!
//! These tests construct a `DaemonApp` directly (without going through the
//! `run_daemon` wrapper or spawning a subprocess) so future in-process test
//! fixtures (B.3) can build on the same surface.

use std::time::Duration;
use tokio::net::TcpListener;

use crate::daemon::{DaemonApp, DaemonConfig, DaemonRuntimeContext};
use crate::daemon::transport::{TransportEndpoint, TransportProbe};
use crate::paths::DaemonPaths;

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

    // After shutdown, the discovery file must have been removed by the MCP
    // transport's own shutdown sequence.
    assert!(
        !discovery_path.exists(),
        "mcp discovery file should be removed during shutdown, still present at {}",
        discovery_path.display()
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
