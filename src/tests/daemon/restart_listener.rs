//! Tests for the restart_notify → stop_notify bridge wired into
//! `DaemonApp::serve`. The bridge consumes `DaemonLifecycleController`'s
//! `restart_notify` channel (previously a dead Notify with no consumer) and
//! forwards every restart signal into the existing SIGTERM exit path via
//! `stop_notify.notify_one()`.
//!
//! The first two tests exercise the helper in isolation, without DaemonApp's
//! TcpListener / dashboard / WatcherPool overhead. The third test
//! (`daemon_exits_within_drain_when_active_session_never_disconnects`)
//! exercises the full end-to-end wiring: a real `DaemonApp::serve` flow
//! where an active HTTP session is kept open across a stale-binary
//! `AcceptWithRestartPending` admission. This proves the daemon exits
//! within the drain timeout instead of hanging in `restart_pending`
//! forever — the production bug this fix addresses.
//!
//! See `docs/plans/2026-05-17-daemon-restart-listener-fix.md` Tasks 2+3.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use serial_test::serial;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::time::timeout;

use crate::daemon::app::spawn_restart_bridge;
use crate::daemon::http_transport::MCP_PATH;
use crate::daemon::mcp_session::{
    HEADER_JULIE_VERSION, HEADER_JULIE_WORKSPACE, HEADER_JULIE_WORKSPACE_SOURCE,
};
use crate::daemon::shutdown::read_recovery_markers;
use crate::daemon::{DaemonApp, DaemonConfig, DaemonRuntimeContext};
use crate::paths::DaemonPaths;
use crate::workspace::startup_hint::WorkspaceStartupSource;

/// The bridge task awakens `stop_notify` once `restart_notify` is signaled
/// after the bridge is spawned. Verifies the normal-case (signal-after-arm)
/// path: notifier fires, bridged stop wake fires within 100ms.
#[tokio::test]
async fn restart_listener_bridge_routes_notify() {
    let restart_notify = Arc::new(Notify::new());
    let stop_notify = Arc::new(Notify::new());

    let _bridge_handle =
        spawn_restart_bridge(Arc::clone(&restart_notify), Arc::clone(&stop_notify));

    // Give the spawned task a moment to arm `.notified()` before we fire.
    // Even without this, `Notify::notify_one` would coalesce a permit
    // (covered by the second test below), but this proves the
    // arrived-after-arm path independently.
    tokio::task::yield_now().await;

    restart_notify.notify_one();

    timeout(Duration::from_millis(100), stop_notify.notified())
        .await
        .expect(
            "stop_notify must wake within 100ms after restart_notify fires; \
             bridge task did not route the signal",
        );
}

/// `Notify::notify_one` is permit-based: if the notifier fires BEFORE any
/// `.notified()` is awaited, the next `.notified()` returns immediately.
/// This proves the startup race is safe — if `notify_restart()` fires from
/// `mark_restart_pending` before `DaemonApp::serve` spawns the bridge, the
/// bridge still picks up the permit on its first poll and wakes
/// `stop_notify`.
#[tokio::test]
async fn restart_listener_handles_pre_spawn_notify() {
    let restart_notify = Arc::new(Notify::new());
    let stop_notify = Arc::new(Notify::new());

    // Fire BEFORE the bridge is spawned. The Notify holds a permit that
    // the bridge's `.notified()` will consume on its first poll.
    restart_notify.notify_one();

    let _bridge_handle =
        spawn_restart_bridge(Arc::clone(&restart_notify), Arc::clone(&stop_notify));

    timeout(Duration::from_millis(100), stop_notify.notified())
        .await
        .expect(
            "stop_notify must wake within 100ms even when restart_notify \
             fired before the bridge was spawned; \
             Notify::notify_one's permit-before-waiter semantics broke",
        );
}

// ---------------------------------------------------------------------------
// Integration test: bounded recovery via the session idle-reaper (NO force-abort)
//
// Load-bearing invariant: the daemon must restart within a bounded time even
// if an active session never sends DELETE. ORIGINALLY this was guaranteed by
// force-aborting the live session on a drain timeout — but that killed real
// in-use sessions and produced the recovery-marker storm. The fix decouples
// arming `restart_pending` from firing the restart (`mark_restart_pending`
// only signals when `active_sessions == 0`) and adds a restart-scoped session
// idle-reaper that evicts genuinely idle sessions so the count can reach zero.
//
// This test proves the NEW contract: a stale-binary `AcceptWithRestartPending`
// arms the restart but does NOT force-abort. The idle session is reaped, the
// count hits zero, the restart fires, and the daemon exits CLEANLY — no drain
// timeout, no recovery marker.
//
// Test sequence:
//  1. Set JULIE_DAEMON_SESSION_IDLE_TIMEOUT_SECS=1 so the reaper acts within
//     ~1s, and JULIE_DAEMON_DRAIN_TIMEOUT_SECS=2 as a guard — if the restart
//     ever fired with a session still present the drain would time out and
//     write a recovery marker, which the final assertion forbids.
//  2. Build DaemonApp with an injected `current_binary_mtime` closure
//     (production hardcodes `super::binary_mtime`; the testing seam in
//     `DaemonConfig::current_binary_mtime` lets us drive the gate).
//  3. Spawn DaemonApp::serve (which starts the session reaper), capture the
//     handle's stop_notify.
//  4. Spawn driver task: stop_notify.notified() → handle.shutdown()
//     (mirrors production's `run_daemon` driver in src/daemon/mod.rs).
//  5. POST initialize for session 1 → Accept (binary not stale yet).
//  6. Flip stale flag.
//  7. POST initialize for session 2 → AcceptWithRestartPending: arms
//     restart_pending but does NOT fire (sessions still active).
//  8. Both sessions are now idle. After ~1s the reaper evicts them; when the
//     count hits zero it calls mark_restart_pending(0), which fires the
//     restart channel → bridge → stop_notify → driver → handle.shutdown().
//     Drain sees an idle tracker → Clean. Daemon exits.
//  9. Assert: driver completes within a bounded budget AND no recovery marker
//     was written (the shutdown was clean, not a force-abort).
// ---------------------------------------------------------------------------

/// Guard that restores a process-wide env var on drop. Mirrors the helper
/// in `src/tests/daemon/drain_timeout.rs`. Tests using this guard MUST be
/// marked `#[serial]` to avoid cross-test pollution.
struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: single-threaded by serial attribute; no other threads read this var.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

/// Send a real HTTP `initialize` request to a daemon's MCP transport.
/// Mirrors the pattern in `src/tests/daemon/http_transport.rs` so the
/// admission path is exercised exactly like production.
///
/// Uses blocking `std::net::TcpStream` because the existing harness does;
/// `#[tokio::test(flavor = "multi_thread", worker_threads = 2)]` provides
/// the second runtime thread so the daemon can still service the request
/// while this call blocks the test's main task.
fn post_initialize(addr: SocketAddr, workspace: &std::path::Path, bearer_token: &str) -> String {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"julie-test","version":"0.0.0"}}}"#;
    let host = format!("127.0.0.1:{}", addr.port());
    let mut request = format!(
        "POST {MCP_PATH} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
         Accept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\
         Authorization: Bearer {bearer_token}\r\n\
         {HEADER_JULIE_WORKSPACE}: {}\r\n\
         {HEADER_JULIE_WORKSPACE_SOURCE}: {}\r\n\
         {HEADER_JULIE_VERSION}: {}\r\n\r\n",
        body.len(),
        workspace.display(),
        WorkspaceStartupSource::Cli.as_header_value(),
        env!("CARGO_PKG_VERSION"),
    );
    request.push_str(body);

    let mut stream = TcpStream::connect(addr).expect("connect to daemon mcp");
    stream
        .write_all(request.as_bytes())
        .expect("write initialize request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read initialize response");
    response
}

/// End-to-end regression: a stale-binary `AcceptWithRestartPending` event with
/// an active session that never disconnects MUST trigger a daemon restart in
/// bounded time — but CLEANLY, by reaping the idle session rather than
/// force-aborting it. The daemon must not hang in `restart_pending` forever,
/// and must not write a recovery marker (which a force-abort would).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn daemon_reaps_idle_session_to_restart_cleanly_when_it_never_disconnects() {
    // Reaper acts within ~1s of idleness so the test is fast. The drain bound
    // is a guard: if the restart ever fired while a session was still present,
    // the drain would time out at 2s and write a recovery marker — which the
    // final assertion forbids.
    let _idle_env = EnvGuard::set("JULIE_DAEMON_SESSION_IDLE_TIMEOUT_SECS", "1");
    let _drain_env = EnvGuard::set("JULIE_DAEMON_DRAIN_TIMEOUT_SECS", "2");

    // Fresh JULIE_HOME so DaemonApp's singleton lock is uncontended and
    // discovery files don't collide with other tests.
    let home = tempfile::tempdir().expect("home tempdir");
    let workspace_root = tempfile::tempdir().expect("workspace tempdir");
    std::fs::create_dir_all(workspace_root.path().join(".julie")).expect("create workspace .julie");

    let paths = DaemonPaths::with_home(home.path().join("julie-home"));
    paths.ensure_dirs().expect("ensure_dirs");

    // Mtime injection. Production reads `super::binary_mtime` (mtime of
    // the live julie-server binary) for both the startup baseline and
    // the per-admission "current" value. `startup_binary_mtime` is fixed
    // when DaemonApp::new captures it from `current_exe()`; we can only
    // control "current" via the seam. Strategy: return values below the
    // captured startup mtime while not stale (Accept), then jump far
    // above startup once flipped (becomes stale → AcceptWithRestartPending
    // when an active session exists).
    let stale_now = Arc::new(AtomicBool::new(false));
    let stale_now_for_closure = Arc::clone(&stale_now);
    let current_binary_mtime: Arc<dyn Fn() -> Option<SystemTime> + Send + Sync> =
        Arc::new(move || {
            if stale_now_for_closure.load(Ordering::SeqCst) {
                // Far-future mtime guarantees `current > startup`.
                Some(SystemTime::now() + Duration::from_secs(3600))
            } else {
                // Pre-startup mtime guarantees NOT stale.
                Some(SystemTime::UNIX_EPOCH)
            }
        });

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mcp listener");
    let local_addr = listener.local_addr().expect("listener local_addr");

    let config = DaemonConfig {
        paths: paths.clone(),
        port: local_addr.port(),
        no_dashboard: true,
        runtime: DaemonRuntimeContext::default(),
        daemon_lock: None,
        current_binary_mtime: Some(current_binary_mtime),
    };

    let app = DaemonApp::new(config).expect("DaemonApp::new");
    let handle = app.serve(listener).await.expect("DaemonApp::serve");
    let stop_notify = handle.stop_notify();

    // Read the bearer token the daemon published so our POSTs authenticate.
    // HttpTransportServer writes it under paths.token_file() with 0600 perms.
    let bearer_token = std::fs::read_to_string(paths.token_file())
        .expect("read bearer token")
        .trim()
        .to_string();

    // Spawn the production-equivalent driver: when stop_notify fires,
    // initiate graceful shutdown. This is exactly the loop in
    // `src/daemon/mod.rs::run_daemon` minus the platform-signal arm.
    let driver = tokio::spawn(async move {
        stop_notify.notified().await;
        handle.shutdown().await
    });

    // Session 1: binary not yet stale → Accept. Session stays in the
    // tracker until we explicitly DELETE it, which we never do.
    let r1 = post_initialize(local_addr, workspace_root.path(), &bearer_token);
    assert!(
        r1.starts_with("HTTP/1.1 200 OK"),
        "session 1 must succeed before staleness flips: {r1}"
    );

    // Binary becomes stale. Next admission sees current > startup.
    stale_now.store(true, Ordering::SeqCst);

    // Session 2: stale=true, active=1, restart_pending=false
    //   → AcceptWithRestartPending. Admission calls
    //   `mark_restart_pending(active=1, RestartRequired)`, which ARMS
    //   restart_pending but does NOT fire (active_sessions > 0 gate). The
    //   session is accepted; no force-abort. Both sessions are now idle.
    let r2 = post_initialize(local_addr, workspace_root.path(), &bearer_token);
    assert!(
        r2.starts_with("HTTP/1.1 200 OK"),
        "session 2 must be accepted with restart pending: {r2}"
    );

    // Now the session idle-reaper takes over. With restart_pending armed and
    // a 1s idle threshold, it evicts both idle sessions within ~1s; when the
    // count reaches zero it calls mark_restart_pending(0), which fires the
    // restart channel → bridge → stop_notify → driver → handle.shutdown().
    //
    // Load-bearing assertion 1: the driver completes within a bounded budget.
    // A hang here means the reaper failed to unblock the pending restart.
    let shutdown_result = timeout(Duration::from_secs(8), driver)
        .await
        .expect(
            "daemon did not exit within the reaper budget; the session idle-reaper \
             failed to evict the idle sessions and fire the pending restart, so the \
             daemon hung in restart_pending",
        )
        .expect("driver task panicked");
    shutdown_result.expect("handle.shutdown() returned an error");

    // Load-bearing assertion 2: the shutdown was CLEAN. The restart fired only
    // after the tracker went idle (reaped), so the drain completed without a
    // timeout and wrote NO recovery marker. The OLD force-abort behavior would
    // have written one — this is the regression guard for the marker storm.
    let markers = read_recovery_markers(&paths);
    assert!(
        markers.is_empty(),
        "stale-binary restart force-aborted a live session instead of reaping it \
         cleanly; {} recovery marker(s) were written",
        markers.len()
    );
}
