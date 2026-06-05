//! Embeddable daemon surface introduced in plan A1.6.
//!
//! `DaemonApp` owns all heavy daemon state (singleton lock, daemon DB, pools,
//! shared services, background tasks). Split from the I/O loop so future
//! in-process test fixtures (B.3) can drive it without subprocess spawning.
//! Public surface (DaemonApp, DaemonConfig, DaemonHandle, DaemonRuntimeContext)
//! is locked by the plan — do not redesign without strategy sign-off.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::dashboard::state::DashboardEvent;
use crate::paths::DaemonPaths;

use super::binary_mtime;
use super::database::DaemonDatabase;
use super::discovery::{AcquireError, DaemonLockGuard};
use super::embedding_service::EmbeddingService;
use super::http_transport::{HttpTransportConfig, HttpTransportServer, generate_bearer_token};
use super::lifecycle::DaemonLifecycleController;
use super::mcp_session::{DaemonSessionDependencies, HttpJulieService, HttpSessionAdmission};
use super::session::SessionTracker;
use super::shutdown::{RecoveryMarker, read_recovery_markers};
use super::watcher_pool::WatcherPool;
use super::workspace_pool::WorkspacePool;

mod handle;
mod runtime;

pub use handle::DaemonHandle;
pub use runtime::DaemonRuntimeContext;

/// Whether the dashboard browser auto-open is suppressed by the environment.
///
/// `julie-daemon` auto-opens the dashboard in a browser on startup as an
/// interactive convenience. The test suite, however, spawns REAL daemons (both
/// directly via `julie-daemon start` and indirectly via the adapter's
/// `spawn_daemon`), so without a guard every daemon-spawning test pops a browser
/// window — dozens during a full run. Suppress the auto-open under the test
/// runner (`NEXTEST`, set by `cargo nextest` / `cargo xtask test`), in CI (`CI`),
/// or when an operator opts out (`JULIE_NO_BROWSER`). A human running
/// `julie daemon` in a terminal (none of these set) still gets the dashboard.
pub(crate) fn dashboard_browser_open_suppressed_by_env() -> bool {
    browser_open_suppressed_from(
        std::env::var_os("NEXTEST").is_some(),
        std::env::var_os("CI").is_some(),
        std::env::var_os("JULIE_NO_BROWSER").is_some(),
    )
}

/// Pure suppression policy: the dashboard browser auto-open is suppressed when
/// ANY of the signals is present. Split out from
/// [`dashboard_browser_open_suppressed_by_env`] so the policy is unit-testable
/// without mutating process-global environment state.
pub(crate) fn browser_open_suppressed_from(
    nextest: bool,
    ci: bool,
    explicit_opt_out: bool,
) -> bool {
    nextest || ci || explicit_opt_out
}

/// Configuration for constructing a `DaemonApp`.
pub struct DaemonConfig {
    /// Filesystem paths for daemon state (PID, lock, port, discovery, logs).
    pub paths: DaemonPaths,
    /// Bound port for the MCP HTTP transport (matches the listener handed to `serve`).
    pub port: u16,
    /// Suppress `opener::open` browser launch for the dashboard.
    pub no_dashboard: bool,
    /// Injectable runtime context. Production uses the singleton-backed
    /// `Default`; tests use `DaemonRuntimeContext::for_test()` for isolation.
    pub runtime: DaemonRuntimeContext,
    /// Pre-acquired daemon singleton lock. When `Some`, `DaemonApp::new`
    /// uses it instead of acquiring its own — this lets `run_daemon`
    /// acquire the lock BEFORE binding any listener, eliminating the
    /// startup thundering herd where N concurrent daemon spawns would
    /// each bind a port + run partial init before the in-app lock check
    /// killed them. Tests and the InProcessDaemon harness pass `None` and
    /// rely on `DaemonApp::new` to acquire the lock itself.
    pub daemon_lock: Option<DaemonLockGuard>,
    /// Testing seam for the current-binary mtime check threaded into
    /// `HttpSessionAdmission`. Production callers pass `None`, which falls
    /// back to `super::binary_mtime` (reads the live julie-server binary).
    /// Tests pass `Some(closure)` to control mtime without touching the
    /// actual binary on disk — needed to exercise stale-binary admission
    /// arms (`AcceptWithRestartPending`, `RejectForRestart`,
    /// `ShutdownForRestart`) end-to-end through `DaemonApp::serve`. Bug-
    /// orthogonal to the restart-listener fix; the bridge in `serve` is
    /// what actually wakes the daemon out of `restart_pending`.
    pub current_binary_mtime: Option<Arc<dyn Fn() -> Option<SystemTime> + Send + Sync>>,
}

/// Heavy daemon state constructed by `DaemonApp::new` and consumed by `serve`.
/// Holds the daemon lock for the lifetime of the daemon and owns all shared
/// pools and services. The lifecycle controller writes the initial
/// `daemon.state = starting` marker on construction.
pub struct DaemonApp {
    paths: DaemonPaths,
    no_dashboard: bool,
    daemon_state_path: PathBuf,
    // Option-wrapped so `serve` can move the lock + reaper into the handle
    // without partial-move conflicts. They're always `Some` in `new`.
    daemon_lock: Option<DaemonLockGuard>,
    lifecycle: DaemonLifecycleController,
    daemon_db: Option<Arc<DaemonDatabase>>,
    watcher_pool: Arc<WatcherPool>,
    workspace_pool: Arc<WorkspacePool>,
    sessions: Arc<SessionTracker>,
    embedding_service: Arc<EmbeddingService>,
    startup_binary_mtime: Option<SystemTime>,
    /// Optional override for the current-binary mtime check used by
    /// `HttpSessionAdmission`. `None` falls back to production
    /// `super::binary_mtime`. See `DaemonConfig::current_binary_mtime`.
    current_binary_mtime_override: Option<Arc<dyn Fn() -> Option<SystemTime> + Send + Sync>>,
    // Reaper task; moved into the handle's abort list to preserve today's
    // `reaper_handle.abort()` ordering ahead of pool shutdown.
    reaper_handle: Option<JoinHandle<()>>,
    /// Recovery markers read from disk at construction time. Surfaced via
    /// `DashboardState` so the `/status` endpoint can show the operator
    /// that a previous daemon shutdown timed out with in-flight requests.
    /// Cloned into the handle so it survives `serve`'s consumption of self.
    recovery_markers: Arc<Vec<RecoveryMarker>>,
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
            daemon_lock: preacquired_lock,
            current_binary_mtime: current_binary_mtime_override,
        } = config;

        paths
            .ensure_dirs()
            .context("Failed to create daemon directories")?;
        let daemon_state_path = paths.daemon_state();

        // Daemon singleton: a kernel-held lock on daemon.lock, released by
        // the OS when this process exits. Prefer the pre-acquired guard from
        // `run_daemon` (which acquires BEFORE binding to avoid the startup
        // thundering herd); fall back to acquiring here for callers that
        // don't pass one in (tests, InProcessDaemon harness). Legacy
        // daemon.pid and daemon.singleton.lock are reserved for migration
        // detection only.
        let daemon_lock = match preacquired_lock {
            Some(guard) => guard,
            None => match DaemonLockGuard::try_acquire(&paths.daemon_lock()) {
                Ok(guard) => guard,
                Err(AcquireError::AlreadyHeld(held)) => {
                    return Err(anyhow::anyhow!(
                        "Another Julie daemon is already running for this JULIE_HOME \
                         (daemon lock held: {}). Exiting without starting a duplicate.",
                        held.path.display()
                    ));
                }
                Err(other) => {
                    return Err(anyhow::anyhow!("{}", other))
                        .context("Failed to acquire daemon lock");
                }
            },
        };

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
        let watcher_pool = Arc::new(WatcherPool::new_with_mutation_gate_registry(
            Duration::from_secs(300),
            Arc::clone(&runtime.mutation_gate_registry),
        ));
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
            daemon_lock: Some(daemon_lock),
            lifecycle,
            daemon_db,
            watcher_pool,
            workspace_pool,
            sessions,
            embedding_service,
            startup_binary_mtime,
            current_binary_mtime_override,
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
        let session_idle_threshold = super::session::session_idle_timeout();
        let session_reaper_tick = session_reaper_interval(session_idle_threshold);
        let session_reaper_handle = spawn_session_idle_reaper(
            Arc::clone(&self.sessions),
            self.lifecycle.clone(),
            session_reaper_tick,
            session_idle_threshold,
        );
        info!(
            idle_threshold_secs = session_idle_threshold.as_secs(),
            interval_ms = session_reaper_tick.as_millis() as u64,
            "Session idle-reaper started (reaps idle sessions only while a restart is pending)"
        );
        let cleanup_sweep_handle = spawn_cleanup_sweep(
            self.daemon_db.as_ref(),
            self.paths.indexes_dir(),
            Arc::clone(&self.workspace_pool),
            Arc::clone(&self.watcher_pool),
        );
        let (stop_notify, shutdown_event_name) = setup_stop_notify(&self.paths);

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
        //
        // Mtime closure: production callers leave
        // `DaemonConfig::current_binary_mtime` as `None` and we use the
        // process-wide `super::binary_mtime` reader. Tests that need to
        // exercise stale-binary admission arms end-to-end through this
        // serve path (e.g. `restart_listener_bridge_routes_via_daemon_app`)
        // pass `Some(Arc<dyn Fn() -> Option<SystemTime>>)` to drive the
        // gate without touching the on-disk binary.
        let current_binary_mtime: Arc<dyn Fn() -> Option<SystemTime> + Send + Sync> = self
            .current_binary_mtime_override
            .take()
            .unwrap_or_else(|| Arc::new(binary_mtime));
        let current_binary_mtime_for_admission = Arc::clone(&current_binary_mtime);
        let http_session_dependencies = Arc::new(
            DaemonSessionDependencies::new(
                Arc::clone(&self.workspace_pool),
                self.daemon_db.clone(),
                Arc::clone(&self.embedding_service),
                self.lifecycle.restart_pending_handle(),
                Some(dashboard_tx.clone()),
                Some(Arc::clone(&watcher_pool_for_handlers)),
                Arc::clone(&self.sessions),
                Arc::clone(&self.runtime.mutation_gate_registry),
            )
            .with_http_admission(HttpSessionAdmission::new(
                self.lifecycle.clone(),
                self.startup_binary_mtime,
                move || current_binary_mtime_for_admission(),
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
        let dashboard_router = crate::dashboard::create_router(dashboard_state, dashboard_config)
            .context("Failed to initialize dashboard templates")?;
        let port_file = self.paths.daemon_port();
        let (dashboard_listener, dashboard_port) =
            bind_dashboard_listener_and_publish(&port_file).await?;
        let dashboard_url = format!("http://localhost:{}", dashboard_port);

        // A1.8: publish the initial discovery.json now that the HTTP transport
        // is bound. This is the file the adapter reads to find the daemon and
        // the file A1.7's `publish_discovery_phase` rewrites at shutdown —
        // without an initial publish here, those phase rewrites silently no-op
        // and adapters cannot observe lifecycle transitions.
        //
        // The bearer token has already been written to disk by
        // `HttpTransportServer::bind_with_listener`; we only record its path.
        // If the transport was bound without a token (test harness), we fall
        // back to `paths.token_file()` as a stable placeholder — the
        // file will not exist, and adapters connecting against an unauthed
        // transport will simply ignore the token.
        let token_path = http_transport
            .token_path()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.paths.token_file());
        let log_path = self.paths.julie_home().join(format!(
            "daemon.log.{}",
            chrono::Local::now().format("%Y-%m-%d")
        ));
        let discovery_record = crate::daemon::discovery::DiscoveryRecord::for_current_process(
            mcp_local_addr.ip().to_string(),
            mcp_local_addr.port(),
            token_path,
            log_path,
        );
        let discovery_path = self.paths.discovery_file();

        self.lifecycle.startup_complete();

        if let Err(error) = crate::daemon::discovery::DiscoveryFile::write_atomic(
            &discovery_path,
            &discovery_record,
        ) {
            let port_path = self.paths.daemon_port();
            let state_path = self.daemon_state_path.clone();
            let _ = std::fs::remove_file(&discovery_path);
            let _ = std::fs::remove_file(&state_path);
            let _ = std::fs::remove_file(&port_path);
            if let Some(handle) = &cleanup_sweep_handle {
                handle.abort();
            }
            idle_sweep_handle.abort();
            if let Err(shutdown_error) =
                http_transport.shutdown_forced(Duration::from_secs(1)).await
            {
                warn!(
                    error = %shutdown_error,
                    "Failed to shut down HTTP transport after discovery publish failure"
                );
            }
            helpers::signal_shutdown_event_waiter(shutdown_event_name.as_deref());
            return Err(error).with_context(|| {
                format!(
                    "Failed to publish initial discovery.json at {}",
                    discovery_path.display()
                )
            });
        }
        info!(
            path = %discovery_path.display(),
            port = mcp_local_addr.port(),
            "Published initial discovery.json (phase=running)"
        );

        info!(
            port = dashboard_port,
            url = %dashboard_url,
            "Dashboard HTTP server started"
        );

        // Auto-open browser unless suppressed. Background task: `opener::open`
        // can shell out for 1-3s on a cold Windows system.
        //
        // The env guard (`dashboard_browser_open_suppressed_by_env`) is what stops
        // the browser-window flood during test runs: the suite spawns REAL daemons
        // (directly and via the adapter), so without it every daemon-spawning test
        // pops a browser window. Suppressed under the test runner / CI / explicit
        // opt-out; the dashboard HTTP server below still runs regardless.
        if !self.no_dashboard && !dashboard_browser_open_suppressed_by_env() {
            let url = dashboard_url.clone();
            tokio::spawn(async move {
                if let Err(e) = opener::open(&url) {
                    warn!("Failed to open browser: {}", e);
                }
            });
        } else if !self.no_dashboard {
            debug!(
                url = %dashboard_url,
                "Dashboard browser auto-open suppressed by environment \
                 (NEXTEST / CI / JULIE_NO_BROWSER); dashboard is still served"
            );
        }

        // Spawn dashboard server as a background task.
        let dashboard_task = tokio::spawn(async move {
            if let Err(e) = axum::serve(dashboard_listener, dashboard_router).await {
                tracing::error!("Dashboard HTTP server error: {}", e);
            }
        });

        // Bridge restart_notify → stop_notify. Before this fix,
        // `DaemonLifecycleController::restart_notify` had no consumer
        // anywhere in src/, so every `notify_restart()` call (from
        // `mark_restart_pending` and the HTTP session disconnect path)
        // fired into a void and the daemon could sit in
        // `restart_pending=true` indefinitely while every new MCP init
        // was rejected. This one-shot listener funnels restart signals
        // into the existing SIGTERM exit path (drain → LIFO teardown →
        // publish_discovery_phase("stopping")). See
        // docs/plans/2026-05-17-daemon-restart-listener-fix.md Task 2.
        //
        // Bridge task is intentionally fire-and-forget: it either fires
        // exactly once and exits, or is aborted with the runtime at
        // daemon shutdown. No DaemonHandle tracking needed — unlike
        // reaper_handle / idle_sweep_handle / cleanup_sweep_handle /
        // embedding_init_handle which have ongoing work and are
        // explicitly aborted at shutdown (see app/handle.rs).
        let _restart_bridge_handle =
            spawn_restart_bridge(self.lifecycle.restart_notify(), Arc::clone(&stop_notify));

        // Background embedding init runs concurrently with HTTP session
        // handling; see helpers.rs for the panic/cancel handling rationale.
        let embedding_init_handle = spawn_embedding_init(
            Arc::clone(&self.embedding_service),
            self.daemon_db.clone(),
            Arc::clone(&watcher_pool_for_handlers),
            self.paths.clone(),
        );

        // ---- Build the handle (owns lock, pools, background tasks) ----
        let daemon_lock = self
            .daemon_lock
            .take()
            .expect("DaemonApp::serve called on app with no daemon lock");
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
            daemon_lock: Some(daemon_lock),
            http_transport: Some(http_transport),
            transport_shutdown_state,
            embedding_service: Arc::clone(&self.embedding_service),
            workspace_pool: Arc::clone(&self.workspace_pool),
            watcher_pool: Arc::clone(&self.watcher_pool),
            sessions: Arc::clone(&self.sessions),
            lifecycle: self.lifecycle.clone(),
            reaper_handle: Some(reaper_handle),
            idle_sweep_handle: Some(idle_sweep_handle),
            session_reaper_handle: Some(session_reaper_handle),
            cleanup_sweep_handle,
            dashboard_task: Some(dashboard_task),
            embedding_init_handle: Some(embedding_init_handle),
            stop_notify: stop_notify_for_shutdown,
            shutdown_event_name,
        })
    }
}

impl Drop for DaemonApp {
    /// Cleanup if `serve` errored before moving resources into the handle.
    fn drop(&mut self) {
        if let Some(handle) = self.reaper_handle.take() {
            handle.abort();
        }
        let _ = self.daemon_lock.take();
    }
}

mod helpers;
pub(crate) use helpers::{
    acquire_or_yield_to_existing_daemon, bind_mcp_listener_with_fallback,
    publish_starting_discovery, shutdown_signal, spawn_embedding_init, spawn_restart_bridge,
};
use helpers::{
    bind_dashboard_listener_and_publish, open_and_migrate_daemon_db, session_reaper_interval,
    setup_stop_notify, spawn_cleanup_sweep, spawn_session_idle_reaper,
};
