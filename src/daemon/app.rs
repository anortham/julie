//! Embeddable daemon surface introduced in plan A1.6.
//!
//! `DaemonApp` owns all heavy daemon state (singleton lock, daemon DB, pools,
//! shared services, background tasks). Split from the I/O loop so future
//! in-process test fixtures (B.3) can drive it without subprocess spawning.
//! Public surface (DaemonApp, DaemonConfig, DaemonHandle, DaemonRuntimeContext)
//! is locked by the plan — do not redesign without strategy sign-off.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tokio::sync::{Notify, broadcast};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::dashboard::state::DashboardEvent;
use crate::paths::DaemonPaths;

use super::database::DaemonDatabase;
use super::embedding_service::EmbeddingService;
use super::http_transport::{HttpTransportConfig, HttpTransportServer, generate_bearer_token};
use super::lifecycle::{DaemonLifecycleController, LifecyclePhase, ShutdownCause};
use super::mcp_session::{DaemonSessionDependencies, HttpJulieService, HttpSessionAdmission};
use super::pid::PidFile;
use super::session::SessionTracker;
use super::singleton::{SingletonLock, SingletonLockError};
use super::watcher_pool::WatcherPool;
use super::workspace_pool::WorkspacePool;
use super::shutdown::{
    DrainOutcome, RecoveryMarker, drain_with_markers, publish_discovery_phase, read_recovery_markers,
};
use super::{ShutdownArtifacts, binary_mtime, drain_timeout, perform_shutdown_sequence};

/// Injectable runtime context. EMPTY for A1.6 — B.1 will populate fields
/// (mutation_gate_registry, tracing_handle, env_overrides, etc.).
#[derive(Default, Clone)]
pub struct DaemonRuntimeContext {
    // Intentionally empty until B.1.
}

/// Configuration for constructing a `DaemonApp`.
pub struct DaemonConfig {
    /// Filesystem paths for daemon state (PID, lock, port, discovery, logs).
    pub paths: DaemonPaths,
    /// Bound port for the MCP HTTP transport (matches the listener handed to `serve`).
    pub port: u16,
    /// Suppress `opener::open` browser launch for the dashboard.
    pub no_dashboard: bool,
    /// Injectable runtime context (empty until B.1).
    pub runtime: DaemonRuntimeContext,
}

/// Heavy daemon state constructed by `DaemonApp::new` and consumed by `serve`.
/// Holds the singleton lock for the lifetime of the daemon and owns all
/// shared pools and services. The lifecycle controller writes the initial
/// `daemon.state = starting` marker on construction.
pub struct DaemonApp {
    paths: DaemonPaths,
    no_dashboard: bool,
    daemon_state_path: PathBuf,
    // Option-wrapped so `serve` can move lock + pid_file + reaper into the
    // handle without partial-move conflicts. They're always `Some` in `new`.
    singleton_lock: Option<SingletonLock>,
    pid_file: Option<PidFile>,
    lifecycle: DaemonLifecycleController,
    daemon_db: Option<Arc<DaemonDatabase>>,
    watcher_pool: Arc<WatcherPool>,
    workspace_pool: Arc<WorkspacePool>,
    sessions: Arc<SessionTracker>,
    embedding_service: Arc<EmbeddingService>,
    startup_binary_mtime: Option<SystemTime>,
    // Reaper task; moved into the handle's abort list to preserve today's
    // `reaper_handle.abort()` ordering ahead of pool shutdown.
    reaper_handle: Option<JoinHandle<()>>,
    /// Recovery markers read from disk at construction time. Surfaced via
    /// `DashboardState` so the `/status` endpoint can show the operator
    /// that a previous daemon shutdown timed out with in-flight requests.
    /// Cloned into the handle so it survives `serve`'s consumption of self.
    recovery_markers: Arc<Vec<RecoveryMarker>>,
    #[allow(dead_code)] // Reserved for B.1
    runtime: DaemonRuntimeContext,
}

impl DaemonApp {
    /// Recovery markers detected at startup (read from the previous daemon
    /// run's `unclean_shutdown.json`). Empty if the previous run drained
    /// cleanly or no daemon ran before this one. Cheap to clone (`Arc`).
    pub fn recovery_markers(&self) -> Arc<Vec<RecoveryMarker>> {
        Arc::clone(&self.recovery_markers)
    }

    /// Acquire the singleton lock, open the daemon DB (migrations + crash
    /// recovery), construct shared state (pools, sessions, embedding service
    /// in `Initializing`), capture binary mtime, set up the lifecycle
    /// controller. Does NOT bind any listening sockets — `serve` does that.
    ///
    /// Errs if the singleton lock is already held, the PID file cannot be
    /// created exclusively, or daemon directories cannot be initialized.
    pub fn new(config: DaemonConfig) -> Result<Self> {
        let DaemonConfig {
            paths,
            port: _,
            no_dashboard,
            runtime,
        } = config;

        paths
            .ensure_dirs()
            .context("Failed to create daemon directories")?;
        let daemon_state_path = paths.daemon_state();

        // Singleton invariant: kernel-enforced layer beneath PID-file management.
        // Acquired BEFORE the PID file so a racing daemon cannot overwrite it.
        // See "577-daemon cascade" regression (2026-05-12) for the rationale.
        let singleton_lock = match SingletonLock::try_acquire(&paths.daemon_singleton_lock()) {
            Ok(guard) => guard,
            Err(SingletonLockError::AlreadyHeld { path }) => {
                return Err(anyhow::anyhow!(
                    "Another Julie daemon is already running for this JULIE_HOME \
                     (singleton lock held: {}). Exiting without starting a duplicate.",
                    path.display()
                ));
            }
            Err(other) => {
                return Err(anyhow::anyhow!("{}", other))
                    .context("Failed to acquire daemon singleton lock");
            }
        };

        // Atomic PID-file creation (O_CREAT|O_EXCL) closes the TOCTOU window
        // between check_running and create.
        let pid_file =
            PidFile::create_exclusive(&paths.daemon_pid()).context("Failed to start daemon")?;
        info!(pid = std::process::id(), "Daemon PID file created");

        let lifecycle = DaemonLifecycleController::new(daemon_state_path.clone());

        // Open the daemon DB and run migrations + crash recovery. None on
        // failure: the daemon continues without persistence.
        let daemon_db = open_and_migrate_daemon_db(&paths);

        // Embedding service starts in `Initializing`; real bootstrap runs as
        // a background task in `serve` (see helpers::spawn_embedding_init).
        let embedding_service = Arc::new(EmbeddingService::initializing());
        info!("Shared embedding service constructed in Initializing state");

        // Binary mtime is still threaded through HttpSessionAdmission today.
        let startup_binary_mtime = binary_mtime();
        match startup_binary_mtime {
            Some(_) => info!("Binary mtime captured for stale-binary detection"),
            None => warn!("Could not determine binary mtime; stale-binary detection disabled"),
        }

        // Watcher pool + reaper come up first; reaper handle is stashed so
        // `serve` can abort it ahead of `WatcherPool::shutdown`.
        let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
        let reaper_handle = watcher_pool.spawn_reaper(Duration::from_secs(60));
        info!("WatcherPool started (grace=300s, reaper=60s)");

        let workspace_pool = Arc::new(WorkspacePool::new(paths.indexes_dir(), daemon_db.clone()));
        let sessions = Arc::new(SessionTracker::new());

        // Read recovery markers from the previous daemon run so the dashboard
        // /status route can surface "last shutdown timed out with N in-flight
        // requests" to the operator. Read is best-effort — corrupt or missing
        // marker file returns an empty Vec.
        let recovery_markers = Arc::new(read_recovery_markers(&paths));
        if !recovery_markers.is_empty() {
            warn!(
                count = recovery_markers.len(),
                "Recovery markers from previous daemon run detected; \
                 surfacing via dashboard /status until cleared"
            );
        }

        Ok(Self {
            paths,
            no_dashboard,
            daemon_state_path,
            singleton_lock: Some(singleton_lock),
            pid_file: Some(pid_file),
            lifecycle,
            daemon_db,
            watcher_pool,
            workspace_pool,
            sessions,
            embedding_service,
            startup_binary_mtime,
            reaper_handle: Some(reaper_handle),
            recovery_markers,
            runtime,
        })
    }

    /// Run the daemon on the provided MCP HTTP listener. Binds the dashboard
    /// on its own auto-assigned port internally. Spawns background tasks
    /// (idle sweeper, cleanup sweeper, embedding init, MCP serve loop).
    /// Returns once MCP transport is bound and reachable; the actual serve
    /// loop runs as a spawned task owned by the `DaemonHandle`.
    pub async fn serve(mut self, listener: TcpListener) -> Result<DaemonHandle> {
        // Idle sweep + cleanup sweep + Windows stop event.
        let watcher_pool_for_handlers = Arc::clone(&self.watcher_pool);
        let idle_threshold = super::workspace_pool::idle_timeout();
        let idle_sweep_handle = self.workspace_pool.spawn_idle_sweep(
            Arc::clone(&watcher_pool_for_handlers),
            Duration::from_secs(60),
            idle_threshold,
        );
        info!(
            idle_threshold_secs = idle_threshold.as_secs(),
            "WorkspacePool idle sweeper started (interval=60s)"
        );
        let cleanup_sweep_handle = spawn_cleanup_sweep(
            self.daemon_db.as_ref(),
            self.paths.indexes_dir(),
            Arc::clone(&self.workspace_pool),
            Arc::clone(&self.watcher_pool),
        );
        let stop_notify = setup_stop_notify(&self.paths);

        // Dashboard server: own auto-assigned port; MCP uses `listener`.
        let dashboard_state = crate::dashboard::state::DashboardState::new_with_watcher_pool(
            Arc::clone(&self.sessions),
            self.daemon_db.clone(),
            self.lifecycle.restart_pending_handle(),
            self.lifecycle.phase_handle(),
            std::time::Instant::now(),
            Some(Arc::clone(&self.embedding_service)),
            Some(Arc::clone(&watcher_pool_for_handlers)),
            Some(Arc::clone(&self.workspace_pool)),
            50, // error buffer capacity
        )
        .with_recovery_markers(Arc::clone(&self.recovery_markers));
        let dashboard_tx: broadcast::Sender<DashboardEvent> = dashboard_state.sender();

        // HTTP MCP transport on the caller-provided listener.
        let http_session_dependencies = Arc::new(
            DaemonSessionDependencies::new(
                Arc::clone(&self.workspace_pool),
                self.daemon_db.clone(),
                Arc::clone(&self.embedding_service),
                self.lifecycle.restart_pending_handle(),
                Some(dashboard_tx.clone()),
                Some(Arc::clone(&watcher_pool_for_handlers)),
                Arc::clone(&self.sessions),
            )
            .with_http_admission(HttpSessionAdmission::new(
                self.lifecycle.clone(),
                self.startup_binary_mtime,
                binary_mtime,
            )),
        );
        let http_service_dependencies = Arc::clone(&http_session_dependencies);

        let http_transport = HttpTransportServer::bind_with_listener(
            listener,
            self.paths.clone(),
            HttpTransportConfig {
                bearer_token: Some(generate_bearer_token()),
                ..HttpTransportConfig::default()
            },
            move || {
                Ok(HttpJulieService::new(Arc::clone(
                    &http_service_dependencies,
                )))
            },
        )
        .await
        .context("Failed to bind HTTP MCP transport")?;
        let mcp_local_addr = http_transport.local_addr();

        let dashboard_config = crate::dashboard::DashboardConfig::default();
        let dashboard_router =
            crate::dashboard::create_router(dashboard_state, dashboard_config)
                .context("Failed to initialize dashboard templates")?;
        let port_file = self.paths.daemon_port();
        let (dashboard_listener, dashboard_port) =
            bind_dashboard_listener_and_publish(&port_file).await?;
        let dashboard_url = format!("http://localhost:{}", dashboard_port);
        info!(
            port = dashboard_port,
            url = %dashboard_url,
            "Dashboard HTTP server started"
        );

        // Auto-open browser unless suppressed. Background task: `opener::open`
        // can shell out for 1-3s on a cold Windows system.
        if !self.no_dashboard {
            let url = dashboard_url.clone();
            tokio::spawn(async move {
                if let Err(e) = opener::open(&url) {
                    warn!("Failed to open browser: {}", e);
                }
            });
        }

        // Spawn dashboard server as a background task.
        let dashboard_task = tokio::spawn(async move {
            if let Err(e) = axum::serve(dashboard_listener, dashboard_router).await {
                tracing::error!("Dashboard HTTP server error: {}", e);
            }
        });

        self.lifecycle.startup_complete();

        // Background embedding init runs concurrently with HTTP session
        // handling; see helpers.rs for the panic/cancel handling rationale.
        let embedding_init_handle = spawn_embedding_init(
            Arc::clone(&self.embedding_service),
            self.daemon_db.clone(),
            Arc::clone(&watcher_pool_for_handlers),
        );

        // ---- Build the handle (owns lock, pid file, pools, background tasks) ----
        let singleton_lock = self
            .singleton_lock
            .take()
            .expect("DaemonApp::serve called on app with no singleton lock");
        let pid_file = self
            .pid_file
            .take()
            .expect("DaemonApp::serve called on app with no pid file");
        let reaper_handle = self
            .reaper_handle
            .take()
            .expect("DaemonApp::serve called on app with no reaper handle");
        let stop_notify_for_shutdown = Arc::clone(&stop_notify);

        let transport_shutdown_state = http_transport.shutdown_state();

        Ok(DaemonHandle {
            local_addr: mcp_local_addr,
            paths: self.paths.clone(),
            daemon_state_path: self.daemon_state_path.clone(),
            singleton_lock: Some(singleton_lock),
            pid_file: Some(pid_file),
            http_transport: Some(http_transport),
            transport_shutdown_state,
            embedding_service: Arc::clone(&self.embedding_service),
            workspace_pool: Arc::clone(&self.workspace_pool),
            watcher_pool: Arc::clone(&self.watcher_pool),
            sessions: Arc::clone(&self.sessions),
            lifecycle: self.lifecycle.clone(),
            reaper_handle: Some(reaper_handle),
            idle_sweep_handle: Some(idle_sweep_handle),
            cleanup_sweep_handle,
            dashboard_task: Some(dashboard_task),
            embedding_init_handle: Some(embedding_init_handle),
            stop_notify: stop_notify_for_shutdown,
        })
    }
}

impl Drop for DaemonApp {
    /// Cleanup if `serve` errored before moving resources into the handle.
    fn drop(&mut self) {
        if let Some(handle) = self.reaper_handle.take() {
            handle.abort();
        }
        if let Some(pid_file) = self.pid_file.take() {
            if let Err(e) = pid_file.cleanup() {
                warn!("Failed to clean up PID file during DaemonApp drop: {}", e);
            }
        }
        let _ = self.singleton_lock.take();
    }
}

/// Handle to a running `DaemonApp`. Query the bound MCP HTTP address and
/// trigger graceful shutdown via `shutdown`. Option-wrapped fields get moved
/// out during shutdown to avoid partial-move conflicts.
pub struct DaemonHandle {
    local_addr: SocketAddr,
    paths: DaemonPaths,
    daemon_state_path: PathBuf,
    singleton_lock: Option<SingletonLock>,
    pid_file: Option<PidFile>,
    http_transport: Option<HttpTransportServer>,
    /// Shared shutdown-state handle for the HTTP transport. Cloned out of
    /// the transport at `serve` time so `shutdown` can flip it to draining /
    /// aborted without holding the transport itself.
    transport_shutdown_state: super::http_transport::TransportShutdownState,
    embedding_service: Arc<EmbeddingService>,
    workspace_pool: Arc<WorkspacePool>,
    watcher_pool: Arc<WatcherPool>,
    sessions: Arc<SessionTracker>,
    lifecycle: DaemonLifecycleController,
    // Reaper aborted BEFORE pool teardown so it cannot race with
    // `WatcherPool::shutdown`.
    reaper_handle: Option<JoinHandle<()>>,
    idle_sweep_handle: Option<JoinHandle<()>>,
    cleanup_sweep_handle: Option<JoinHandle<()>>,
    dashboard_task: Option<JoinHandle<()>>,
    embedding_init_handle: Option<JoinHandle<()>>,
    // Shared with the Windows shutdown-event waker task.
    #[allow(dead_code)]
    stop_notify: Arc<Notify>,
}

impl DaemonHandle {
    /// Bound MCP HTTP address (the one from the listener passed to `serve`).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
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
            drain_timed_out,
            "Daemon shutting down"
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
        let pid_file = self
            .pid_file
            .take()
            .expect("DaemonHandle::shutdown called twice");
        let http_transport = self
            .http_transport
            .take()
            .expect("DaemonHandle::shutdown called twice");
        let artifacts = ShutdownArtifacts {
            port_path: &port_path,
            pid_file,
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

        // Release singleton lock LAST so a racing daemon cannot start before
        // shared resources are released.
        let _ = self.singleton_lock.take();

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
        if let Some(pid_file) = self.pid_file.take() {
            if let Err(e) = pid_file.cleanup() {
                warn!("Failed to clean up PID file during DaemonHandle drop: {}", e);
            }
        }
        let _ = self.singleton_lock.take();
        let _ = std::fs::remove_file(self.paths.daemon_port());
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

mod helpers;
pub(crate) use helpers::{bind_mcp_listener_with_fallback, shutdown_signal};
use helpers::{
    bind_dashboard_listener_and_publish, open_and_migrate_daemon_db, setup_stop_notify,
    spawn_cleanup_sweep, spawn_embedding_init,
};