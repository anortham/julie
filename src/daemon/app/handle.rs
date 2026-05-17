use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{error, info};

use crate::paths::DaemonPaths;

use super::super::discovery::DaemonLockGuard;
use super::super::embedding_service::EmbeddingService;
use super::super::http_transport::{HttpTransportServer, TransportShutdownState};
use super::super::lifecycle::{DaemonLifecycleController, LifecyclePhase, ShutdownCause};
use super::super::session::SessionTracker;
use super::super::shutdown::{DrainOutcome, drain_with_markers, publish_discovery_phase};
use super::super::watcher_pool::WatcherPool;
use super::super::workspace_pool::WorkspacePool;
use super::super::{ShutdownArtifacts, drain_timeout, perform_shutdown_sequence};

/// Handle to a running `DaemonApp`. Query the bound MCP HTTP address and
/// trigger graceful shutdown via `shutdown`. Option-wrapped fields get moved
/// out during shutdown to avoid partial-move conflicts.
pub struct DaemonHandle {
    pub(super) local_addr: SocketAddr,
    pub(super) paths: DaemonPaths,
    pub(super) daemon_state_path: PathBuf,
    pub(super) daemon_lock: Option<DaemonLockGuard>,
    pub(super) http_transport: Option<HttpTransportServer>,
    /// Shared shutdown-state handle for the HTTP transport. Cloned out of
    /// the transport at `serve` time so `shutdown` can flip it to draining /
    /// aborted without holding the transport itself.
    pub(super) transport_shutdown_state: TransportShutdownState,
    pub(super) embedding_service: Arc<EmbeddingService>,
    pub(super) workspace_pool: Arc<WorkspacePool>,
    pub(super) watcher_pool: Arc<WatcherPool>,
    pub(super) sessions: Arc<SessionTracker>,
    pub(super) lifecycle: DaemonLifecycleController,
    // Reaper aborted BEFORE pool teardown so it cannot race with
    // `WatcherPool::shutdown`.
    pub(super) reaper_handle: Option<JoinHandle<()>>,
    pub(super) idle_sweep_handle: Option<JoinHandle<()>>,
    pub(super) cleanup_sweep_handle: Option<JoinHandle<()>>,
    pub(super) dashboard_task: Option<JoinHandle<()>>,
    pub(super) embedding_init_handle: Option<JoinHandle<()>>,
    // Shared with the Windows shutdown-event waker task.
    #[allow(dead_code)]
    pub(super) stop_notify: Arc<Notify>,
}

impl DaemonHandle {
    /// Bound MCP HTTP address (the one from the listener passed to `serve`).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub(crate) fn stop_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.stop_notify)
    }

    /// Trigger graceful shutdown and wait for the serve task to complete.
    ///
    /// LIFO order: flip transport to 503 (draining), rewrite discovery.json
    /// `phase=stopping`, drain sessions, on timeout flip transport to 502
    /// (aborted) and persist recovery markers, abort background tasks, shut
    /// down HTTP transport, embedding service, workspace pool, watcher
    /// pool, then remove port/PID/state files and release the singleton
    /// lock. See `perform_shutdown_sequence` in `daemon/mod.rs` for steps
    /// 3-5.
    pub async fn shutdown(mut self) -> Result<()> {
        // Step 0: announce that we're shutting down.
        // (a) Flip HTTP transport into the draining state so any NEW request
        //     after this point bounces with 503 Retry-After. In-flight
        //     requests continue running and are observed by the drain below.
        // (b) Rewrite discovery.json with phase=stopping so adapters / the
        //     dashboard reading the file see the lifecycle change without
        //     waiting for the transport to fully tear down. (No-op until
        //     A1.8 publishes the initial discovery.json.)
        self.transport_shutdown_state.mark_draining();
        publish_discovery_phase(&self.paths, "stopping");

        let remaining = self.sessions.active_count();
        let phase = self
            .lifecycle
            .request_shutdown(ShutdownCause::Signal, remaining);
        let drain_timed_out = if remaining > 0 {
            let timeout = drain_timeout();
            info!(
                active_sessions = remaining,
                timeout_secs = timeout.as_secs(),
                "Draining active sessions"
            );
            let outcome = drain_with_markers(&self.sessions, &self.paths, timeout).await;
            match outcome {
                DrainOutcome::Clean => {
                    info!("All sessions drained cleanly");
                    false
                }
                DrainOutcome::TimedOut { active_sessions } => {
                    // Flip the transport into the aborted state so any
                    // in-flight handler that tries to write a response after
                    // this point gets a 502 instead of a torn-down resource
                    // (e.g. a dropped sidecar). The recovery marker has
                    // already been written by drain_with_markers.
                    self.transport_shutdown_state.mark_aborted();
                    error!(
                        remaining = active_sessions,
                        "Session drain timeout exceeded, force-aborting in-flight requests — \
                         recovery marker written for the next startup"
                    );
                    true
                }
            }
        } else {
            false
        };
        if matches!(phase, LifecyclePhase::Draining { .. }) {
            self.lifecycle.sessions_drained();
        }

        info!(
            active_sessions = self.sessions.active_count(),
            drain_timed_out, "Daemon shutting down"
        );

        // Abort background tasks before tearing down shared resources to
        // prevent races with explicit shutdown calls. Order matches today's
        // run_daemon: reaper, idle sweep, cleanup sweep, serve-spawned tasks.
        for handle in [
            self.reaper_handle.take(),
            self.idle_sweep_handle.take(),
            self.cleanup_sweep_handle.take(),
            self.dashboard_task.take(),
            self.embedding_init_handle.take(),
        ]
        .into_iter()
        .flatten()
        {
            handle.abort();
        }

        // LIFO shutdown via perform_shutdown_sequence (HTTP transport first).
        let port_path = self.paths.daemon_port();
        let discovery_path = self.paths.discovery_file();
        let http_transport = self
            .http_transport
            .take()
            .expect("DaemonHandle::shutdown called twice");
        let artifacts = ShutdownArtifacts {
            port_path: &port_path,
            discovery_path: &discovery_path,
            state_path: &self.daemon_state_path,
        };
        perform_shutdown_sequence(
            http_transport,
            Arc::clone(&self.embedding_service),
            Arc::clone(&self.workspace_pool),
            Arc::clone(&self.watcher_pool),
            artifacts,
            None,
        )
        .await;

        // Release daemon lock LAST so a racing daemon cannot start before
        // shared resources are released.
        let _ = self.daemon_lock.take();

        info!("Daemon stopped");
        Ok(())
    }
}

impl Drop for DaemonHandle {
    /// Best-effort cleanup if the handle is dropped without an explicit
    /// `shutdown` call (e.g. panic / `?` early-exit). Cleans up PID file,
    /// singleton lock, state files, and background tasks. The async HTTP
    /// transport shutdown can't run here; its cancellation token + JoinHandle
    /// abort handle the unclean path.
    fn drop(&mut self) {
        let _ = self.daemon_lock.take();
        let _ = std::fs::remove_file(self.paths.daemon_port());
        let _ = std::fs::remove_file(self.paths.discovery_file());
        // Symmetry with the graceful `shutdown` path (HttpTransportServer
        // removes the token on its own drop). Without this, a panic/early
        // return before `shutdown` leaves the 0600 bearer-token file behind.
        let _ = std::fs::remove_file(self.paths.token_file());
        let _ = std::fs::remove_file(&self.daemon_state_path);
        for h in [
            self.reaper_handle.take(),
            self.idle_sweep_handle.take(),
            self.cleanup_sweep_handle.take(),
            self.dashboard_task.take(),
            self.embedding_init_handle.take(),
        ]
        .into_iter()
        .flatten()
        {
            h.abort();
        }
    }
}
