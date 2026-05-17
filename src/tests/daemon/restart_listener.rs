//! Tests for the restart_notify → stop_notify bridge wired into
//! `DaemonApp::serve`. The bridge consumes `DaemonLifecycleController`'s
//! `restart_notify` channel (previously a dead Notify with no consumer) and
//! forwards every restart signal into the existing SIGTERM exit path via
//! `stop_notify.notify_one()`.
//!
//! These tests exercise the helper in isolation, without DaemonApp's
//! TcpListener / dashboard / WatcherPool overhead. The full end-to-end
//! wiring (active session never disconnects, daemon exits within drain
//! timeout) is exercised by Task 3.
//!
//! See `docs/plans/2026-05-17-daemon-restart-listener-fix.md` Task 2.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::timeout;

use crate::daemon::app::spawn_restart_bridge;

/// The bridge task awakens `stop_notify` once `restart_notify` is signaled
/// after the bridge is spawned. Verifies the normal-case (signal-after-arm)
/// path: notifier fires, bridged stop wake fires within 100ms.
#[tokio::test]
async fn restart_listener_bridge_routes_notify() {
    let restart_notify = Arc::new(Notify::new());
    let stop_notify = Arc::new(Notify::new());

    let _bridge_handle = spawn_restart_bridge(
        Arc::clone(&restart_notify),
        Arc::clone(&stop_notify),
    );

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

    let _bridge_handle = spawn_restart_bridge(
        Arc::clone(&restart_notify),
        Arc::clone(&stop_notify),
    );

    timeout(Duration::from_millis(100), stop_notify.notified())
        .await
        .expect(
            "stop_notify must wake within 100ms even when restart_notify \
             fired before the bridge was spawned; \
             Notify::notify_one's permit-before-waiter semantics broke",
        );
}
