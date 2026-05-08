//! Julie daemon: persistent background process serving MCP sessions.
//!
//! The canonical adapter path is Streamable HTTP over localhost.

pub mod database;
pub mod embedding_service;
pub mod http_client;
pub mod http_transport;
pub mod lifecycle;
pub mod mcp_session;
pub mod pid;
pub mod project_log;
pub mod session;
#[cfg(windows)]
pub mod shutdown_event;
pub mod transport;
pub mod watcher_pool;
pub mod workspace_pool;
pub mod workspace_registry_store;
pub mod workspace_session_attachment;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use tokio::sync::{Notify, broadcast};
use tracing::{error, info, warn};

use crate::dashboard::state::DashboardEvent;

use crate::paths::DaemonPaths;
use crate::workspace::registry::generate_workspace_id;

use self::database::DaemonDatabase;
use self::embedding_service::EmbeddingService;
use self::http_transport::{HttpTransportConfig, HttpTransportServer, generate_bearer_token};
use self::lifecycle::{DaemonLifecycleController, LifecyclePhase, ShutdownCause};
use self::mcp_session::{DaemonSessionDependencies, HttpJulieService, HttpSessionAdmission};
use self::pid::PidFile;
use self::session::SessionTracker;
use self::watcher_pool::WatcherPool;
use self::workspace_pool::WorkspacePool;
use self::workspace_registry_store::WorkspaceRegistryStore;

/// Wait for all active daemon sessions to finish, with a deadline.
///
/// Returns `true` if sessions drained cleanly, `false` if the timeout elapsed
/// while sessions were still active.
pub(crate) async fn drain_sessions(sessions: &SessionTracker, timeout: Duration) -> bool {
    tokio::time::timeout(timeout, async {
        loop {
            // Arm the notifier before checking count to avoid missing a wake-up
            // between the check and the await (standard condvar pattern).
            let notified = sessions.session_notify().notified();
            if sessions.is_idle() {
                return;
            }
            notified.await;
        }
    })
    .await
    .is_ok()
}

const DRAIN_TIMEOUT_ENV: &str = "JULIE_DAEMON_DRAIN_TIMEOUT_SECS";
const DEFAULT_DRAIN_TIMEOUT_SECS: u64 = 10;
const MIN_DRAIN_TIMEOUT_SECS: u64 = 1;
const MAX_DRAIN_TIMEOUT_SECS: u64 = 120;

/// Return the configured drain timeout for the daemon shutdown sequence.
///
/// Reads `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` from the environment. Valid range
/// is [1, 120] seconds. Values outside this range, or unparseable strings,
/// fall back to the default (10 s) with a warning. The default is intentionally
/// larger than the old 5 s literal to handle Windows NTFS fsync latency during
/// stale-binary restarts.
pub(crate) fn drain_timeout() -> Duration {
    match std::env::var(DRAIN_TIMEOUT_ENV) {
        Ok(s) => match s.parse::<u64>() {
            Ok(v) if (MIN_DRAIN_TIMEOUT_SECS..=MAX_DRAIN_TIMEOUT_SECS).contains(&v) => {
                Duration::from_secs(v)
            }
            Ok(v) => {
                warn!(
                    value = v,
                    "JULIE_DAEMON_DRAIN_TIMEOUT_SECS out of range [1,120]; using default {}s",
                    DEFAULT_DRAIN_TIMEOUT_SECS
                );
                Duration::from_secs(DEFAULT_DRAIN_TIMEOUT_SECS)
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "JULIE_DAEMON_DRAIN_TIMEOUT_SECS unparseable; using default {}s",
                    DEFAULT_DRAIN_TIMEOUT_SECS
                );
                Duration::from_secs(DEFAULT_DRAIN_TIMEOUT_SECS)
            }
        },
        Err(_) => Duration::from_secs(DEFAULT_DRAIN_TIMEOUT_SECS),
    }
}

/// Capture the binary's mtime so we can detect a replacement at runtime.
///
/// Used at daemon startup to snapshot the binary mtime, then compared on each
/// session disconnect to detect whether the binary has been rebuilt. If it has,
/// the daemon exits after the last session disconnects so the adapter can
/// restart it with the new binary.
///
/// Note (Windows): the running `julie-server.exe` holds an exclusive image-section
/// lock on its own binary, so a developer running `cargo build --release` against
/// a live daemon FAILS with "Access is denied" rather than producing a new binary
/// the daemon could see. Stale-binary detection therefore fires for: (a) installers
/// that use `MoveFileEx(MOVEFILE_REPLACE_EXISTING)`, (b) `touch`-style mtime bumps
/// without byte changes, (c) a delete + new-name + rename sequence done out of band.
/// It does NOT fire for in-place developer rebuilds on Windows; the developer must
/// stop the daemon first.
fn binary_mtime() -> Option<SystemTime> {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}

/// Backfill vector_count in daemon.db for all workspaces with embeddings on disk.
///
/// Scans each workspace's symbols.db for stored embeddings and writes the count
/// to daemon.db if missing. Runs once at daemon startup so the dashboard shows
/// accurate vector counts without waiting for a session to connect.
fn backfill_all_vector_counts(daemon_db: &DaemonDatabase, indexes_dir: &Path) {
    let workspaces = match daemon_db.list_workspaces() {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let mut count = 0;
    for ws in &workspaces {
        if ws.vector_count.is_some() {
            continue;
        }
        let db_path = indexes_dir
            .join(&ws.workspace_id)
            .join("db")
            .join("symbols.db");
        if !db_path.exists() {
            continue;
        }
        // Open the symbols.db read-only and query embedding count
        let vectors = match crate::database::SymbolDatabase::new(&db_path) {
            Ok(db) => db.embedding_count().unwrap_or(0),
            Err(_) => continue,
        };
        if vectors > 0 {
            let _ = daemon_db.update_vector_count(&ws.workspace_id, vectors);
            count += 1;
        }
    }
    if count > 0 {
        info!(count, "Backfilled vector_count for workspaces");
    }
}

/// Reconcile workspace IDs after a normalize_path behavior change.
///
/// Compares each workspace's stored ID against the current generate_workspace_id
/// output. If they differ, renames the index directory and batch-updates the DB.
fn migrate_stale_workspace_ids(daemon_db: &DaemonDatabase, indexes_dir: &Path) {
    let workspaces = match daemon_db.list_workspaces() {
        Ok(ws) => ws,
        Err(e) => {
            warn!("Failed to list workspaces for migration check: {}", e);
            return;
        }
    };

    // Phase 1: Compute all ID mappings
    let mut id_map = std::collections::HashMap::new();
    for ws in &workspaces {
        // Clean up root-path artifact
        if ws.path == "/" {
            info!(
                workspace_id = %ws.workspace_id,
                "Removing stale root-path workspace entry"
            );
            if let Err(e) = daemon_db.delete_workspace(&ws.workspace_id) {
                warn!("Failed to delete root workspace: {}", e);
            }
            let root_dir = indexes_dir.join(&ws.workspace_id);
            if root_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&root_dir) {
                    warn!("Failed to remove root workspace dir: {}", e);
                }
            }
            continue;
        }

        match generate_workspace_id(&ws.path) {
            Ok(new_id) if new_id != ws.workspace_id => {
                info!(
                    old_id = %ws.workspace_id,
                    new_id = %new_id,
                    path = %ws.path,
                    "Workspace ID needs migration"
                );
                id_map.insert(ws.workspace_id.clone(), new_id);
            }
            Err(e) => {
                warn!(
                    workspace_id = %ws.workspace_id,
                    path = %ws.path,
                    "Failed to regenerate workspace ID: {}", e
                );
            }
            _ => {} // ID matches, no migration needed
        }
    }

    if id_map.is_empty() {
        return;
    }

    // Phase 2: Rename/delete index directories
    let mut disk_failures: Vec<String> = Vec::new();
    for (old_id, new_id) in &id_map {
        let old_dir = indexes_dir.join(old_id);
        let new_dir = indexes_dir.join(new_id);

        if old_dir.exists() && !new_dir.exists() {
            if let Err(e) = std::fs::rename(&old_dir, &new_dir) {
                warn!(
                    old_id,
                    new_id,
                    "Failed to rename index dir, skipping DB migration for this entry: {}",
                    e
                );
                disk_failures.push(old_id.clone());
            } else {
                info!(old_id, new_id, "Renamed index directory");
            }
        } else if old_dir.exists() && new_dir.exists() {
            // Both exist: new is active (created by post-fix code), old is stale
            if let Err(e) = std::fs::remove_dir_all(&old_dir) {
                warn!(old_id, "Failed to remove stale index dir: {}", e);
            } else {
                info!(
                    old_id,
                    "Removed stale index directory (new dir already exists)"
                );
            }
        }
    }

    // Remove entries where disk operations failed
    for failed_id in &disk_failures {
        id_map.remove(failed_id);
    }

    if id_map.is_empty() {
        return;
    }

    // Phase 3: Batch-update DB
    match daemon_db.migrate_workspace_ids(&id_map) {
        Ok(()) => {
            info!(
                count = id_map.len(),
                "Successfully migrated workspace IDs in daemon.db"
            );
        }
        Err(e) => {
            warn!("Failed to migrate workspace IDs in DB: {}", e);
        }
    }
}

/// Run the Julie daemon: bind HTTP transport, accept connections, serve MCP.
///
/// This function blocks until a shutdown signal (SIGTERM/SIGINT) is received.
/// HTTP is the daemon MCP transport.
pub async fn run_daemon(paths: DaemonPaths, port: u16, no_dashboard: bool) -> Result<()> {
    paths
        .ensure_dirs()
        .context("Failed to create daemon directories")?;
    let daemon_state_path = paths.daemon_state();

    // Atomically check-and-create the PID file. create_exclusive uses O_CREAT|O_EXCL
    // internally, eliminating the TOCTOU window between check_running and create
    // that allowed two concurrent invocations to both believe they were first.
    let pid_file =
        PidFile::create_exclusive(&paths.daemon_pid()).context("Failed to start daemon")?;
    info!(pid = std::process::id(), "Daemon PID file created");
    let lifecycle = DaemonLifecycleController::new(daemon_state_path.clone());

    // Open persistent daemon database, resetting stale session counts from
    // any previous run (crash recovery) and pruning old tool call records.
    let daemon_db: Option<Arc<DaemonDatabase>> = match DaemonDatabase::open(&paths.daemon_db()) {
        Ok(db) => {
            if let Err(e) = db.reset_all_session_counts() {
                warn!("Failed to reset session counts: {}", e);
            }
            if let Err(e) = db.prune_tool_calls(90) {
                warn!("Failed to prune old tool calls: {}", e);
            }
            info!("Daemon database ready: {}", paths.daemon_db().display());
            Some(Arc::new(db))
        }
        Err(e) => {
            warn!(
                "Failed to open daemon.db, continuing without persistence: {}",
                e
            );
            None
        }
    };

    // Migrate stale workspace IDs from pre-v6.0.4 normalize_path behavior.
    // Must run before WorkspacePool is created so sessions see correct IDs.
    if let Some(ref db) = daemon_db {
        migrate_stale_workspace_ids(db, &paths.indexes_dir());

        // Normalize path separators (fixes adapter's previous forward-slash storage)
        // and restore "ready" status for workspaces stuck at "pending".
        match db.normalize_workspace_paths() {
            Ok(0) => {}
            Ok(n) => info!(count = n, "Normalized workspace paths in daemon.db"),
            Err(e) => warn!("Failed to normalize workspace paths: {}", e),
        }

        // Backfill vector_count for workspaces that have embeddings but no count
        // in daemon.db (handles workspaces embedded before this stat was tracked).
        backfill_all_vector_counts(db, &paths.indexes_dir());
    }

    // Construct the shared embedding service in `Initializing` state. The
    // real provider bootstrap (Python sidecar + PyTorch + CodeRankEmbed model
    // load, ~36-39s on typical hardware) runs as a background task spawned
    // below, after HTTP transport is bound and `ready` state is published.
    // This keeps the daemon off the critical path so MCP clients (e.g.
    // Claude Code, whose MCP_TIMEOUT defaults to 30s) don't time out on the
    // first connection after a cold start. See
    // docs/plans/2026-04-09-daemon-lazy-embedding-init-design.md for the
    // full rationale.
    let embedding_service = Arc::new(EmbeddingService::initializing());
    info!(
        "Shared embedding service constructed in Initializing state; background init will start after HTTP transport bind"
    );

    // Capture binary mtime at startup for stale-binary detection.
    // If the binary is rebuilt while the daemon is running, the next session
    // disconnect will detect the mismatch and trigger a graceful restart.
    let startup_binary_mtime = binary_mtime();
    if startup_binary_mtime.is_some() {
        info!("Binary mtime captured for stale-binary detection");
    } else {
        warn!("Could not determine binary mtime; stale-binary detection disabled");
    }

    // Shared state
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let reaper_handle = watcher_pool.spawn_reaper(Duration::from_secs(60));
    info!("WatcherPool started (grace=300s, reaper=60s)");
    // Keep a clone so per-session handlers can pause/resume non-primary workspace watchers.
    let watcher_pool_for_handlers = Arc::clone(&watcher_pool);
    let watcher_pool_for_cleanup = Arc::clone(&watcher_pool);

    let pool = Arc::new(WorkspacePool::new(paths.indexes_dir(), daemon_db.clone()));
    let cleanup_pool = Arc::clone(&pool);
    let sessions = Arc::new(SessionTracker::new());

    let cleanup_sweep_handle = daemon_db.as_ref().map(|daemon_db| {
        let registry_store =
            WorkspaceRegistryStore::new(Arc::clone(daemon_db), paths.indexes_dir());
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(600));
            loop {
                tick.tick().await;
                let cleanup_activity =
                    crate::tools::workspace::commands::registry::cleanup::WorkspaceCleanupActivity::new(
                        Some(&cleanup_pool),
                        Some(&watcher_pool_for_cleanup),
                    );
                match crate::tools::workspace::commands::registry::cleanup::run_cleanup_sweep(
                    &registry_store,
                    &cleanup_activity,
                )
                .await
                {
                    Ok(summary) => {
                        if !summary.pruned_workspaces.is_empty()
                            || !summary.pruned_orphan_dirs.is_empty()
                        {
                            info!(
                                pruned_workspaces = summary.pruned_workspaces.len(),
                                pruned_orphan_dirs = summary.pruned_orphan_dirs.len(),
                                blocked_workspaces = summary.blocked_workspaces.len(),
                                "Background workspace cleanup sweep removed stale entries"
                            );
                        }
                    }
                    Err(e) => {
                        warn!("Background workspace cleanup sweep failed: {}", e);
                    }
                }
            }
        })
    });

    // Named event for graceful shutdown from `julie stop` / `julie restart`.
    // On Windows, ctrl_c() requires a console (which CREATE_NO_WINDOW daemons
    // lack), so this named event is the primary graceful shutdown mechanism.
    let stop_notify = Arc::new(Notify::new());
    #[cfg(windows)]
    {
        let event_name = paths.daemon_shutdown_event();
        match shutdown_event::ShutdownEvent::create(&event_name) {
            Ok(event) => {
                info!("Shutdown event created: {}", event_name);
                let notify = Arc::clone(&stop_notify);
                let event = Arc::new(event);
                tokio::task::spawn_blocking(move || {
                    event.wait();
                    notify.notify_one();
                });
            }
            Err(e) => {
                warn!(
                    "Failed to create shutdown event: {}. \
                     Graceful stop via `julie stop` unavailable.",
                    e
                );
            }
        }
    }

    // --- Dashboard HTTP server ---
    // Pass the EmbeddingService Arc directly so DashboardState reads its
    // state live. With lazy init, the service starts in Initializing and
    // transitions to Ready (or Unavailable) once the background task
    // finishes, and the dashboard reflects this without a restart.
    let dashboard_state = crate::dashboard::state::DashboardState::new_with_watcher_pool(
        Arc::clone(&sessions),
        daemon_db.clone(),
        lifecycle.restart_pending_handle(),
        lifecycle.phase_handle(),
        std::time::Instant::now(),
        Some(Arc::clone(&embedding_service)),
        Some(Arc::clone(&watcher_pool_for_handlers)),
        Some(Arc::clone(&pool)),
        50, // error buffer capacity
    );

    // Extract the broadcast sender before dashboard_state is moved into the router.
    // Cloned cheaply (Arc-backed) and passed to each HTTP session for live-feed events.
    let dashboard_tx: broadcast::Sender<DashboardEvent> = dashboard_state.sender();

    let http_session_dependencies = Arc::new(
        DaemonSessionDependencies::new(
            Arc::clone(&pool),
            daemon_db.clone(),
            Arc::clone(&embedding_service),
            lifecycle.restart_pending_handle(),
            Some(dashboard_tx.clone()),
            Some(Arc::clone(&watcher_pool_for_handlers)),
            Arc::clone(&sessions),
        )
        .with_http_admission(HttpSessionAdmission::new(
            lifecycle.clone(),
            startup_binary_mtime,
            binary_mtime,
        )),
    );
    let http_service_dependencies = Arc::clone(&http_session_dependencies);
    let http_transport = HttpTransportServer::bind(
        paths.clone(),
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

    let dashboard_config = crate::dashboard::DashboardConfig::default();
    let dashboard_router = crate::dashboard::create_router(dashboard_state, dashboard_config)
        .context("Failed to initialize dashboard templates")?;

    // Try requested port, fall back to auto-assign
    let http_listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await {
        Ok(l) => l,
        Err(_) if port != 0 => {
            warn!("Port {} in use, falling back to auto-assign", port);
            tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .context("Failed to bind HTTP server on any port")?
        }
        Err(e) => return Err(anyhow::anyhow!("Failed to bind HTTP server: {}", e)),
    };

    let actual_port = http_listener.local_addr()?.port();

    // Write port file so `julie dashboard` can find it
    let port_file = paths.daemon_port();
    std::fs::write(&port_file, actual_port.to_string())
        .context("Failed to write daemon port file")?;

    let dashboard_url = format!("http://localhost:{}", actual_port);
    info!(port = actual_port, url = %dashboard_url, "Dashboard HTTP server started");

    // Auto-open browser unless suppressed. Runs in a background task so
    // `opener::open` (which shells out to `cmd /c start <url>` on Windows
    // and can take 1-3s on a cold system) doesn't block daemon readiness.
    // Browser launch is purely a UX nicety.
    if !no_dashboard {
        let url = dashboard_url.clone();
        tokio::spawn(async move {
            if let Err(e) = opener::open(&url) {
                warn!("Failed to open browser: {}", e);
            }
        });
    }

    // Spawn HTTP server as background task
    tokio::spawn(async move {
        if let Err(e) = axum::serve(http_listener, dashboard_router).await {
            tracing::error!("Dashboard HTTP server error: {}", e);
        }
    });

    lifecycle.startup_complete();

    // Spawn the background embedding provider initialization task. This runs
    // concurrently with HTTP session handling so the daemon becomes ready
    // quickly even though `create_embedding_provider` itself can take tens of seconds
    // (Python sidecar + torch + model load). Downstream callers that need
    // the provider (spawn_workspace_embedding, nl_embeddings, watchers, the
    // dashboard) are all daemon-mode aware and wait on
    // `EmbeddingService::wait_until_settled` with a bounded timeout rather
    // than hanging indefinitely. See Task 2 of
    // docs/plans/2026-04-09-daemon-lazy-embedding-init.md for the rationale
    // and failure-mode analysis, especially the `Err(join_err)` arm. That
    // arm is critical: without it, a panicking init task would leave the
    // service stuck in `Initializing` forever and every future
    // `wait_until_settled` would time out rather than report the real
    // failure.
    {
        let embedding_service_for_init = Arc::clone(&embedding_service);
        let daemon_db_for_init = daemon_db.clone();
        let watcher_pool_for_init = Arc::clone(&watcher_pool_for_handlers);
        tokio::spawn(async move {
            info!("Background embedding init task started");
            let init_result =
                tokio::task::spawn_blocking(|| crate::embeddings::create_embedding_provider())
                    .await;

            match init_result {
                Ok((Some(provider), Some(status))) => {
                    let model_name = provider.device_info().model_name.clone();
                    embedding_service_for_init.publish_ready(Arc::clone(&provider), status);

                    // Propagate the provider to any watchers that were
                    // attached during the warmup window. They start with
                    // None in their SharedEmbeddingProvider RwLock cell
                    // (because shared_embedding_provider() returned None
                    // while the service was Initializing), and would never
                    // see the new provider without this push. Watchers
                    // attached AFTER publish_ready get the provider via
                    // their normal attach path, so this only matters for
                    // the warmup race.
                    watcher_pool_for_init
                        .update_all_provider(Some(Arc::clone(&provider)))
                        .await;

                    // Sync embedding_model for workspaces that have vectors
                    // but a missing or stale model name. Previously ran on the
                    // critical path right after EmbeddingService::initialize;
                    // now it runs here, once the background init actually
                    // produces a provider.
                    if let Some(ref db) = daemon_db_for_init {
                        if let Ok(workspaces) = db.list_workspaces() {
                            let mut count = 0;
                            for ws in &workspaces {
                                if ws.vector_count.map_or(false, |v| v > 0)
                                    && ws.embedding_model.as_deref() != Some(model_name.as_str())
                                {
                                    let _ =
                                        db.update_embedding_model(&ws.workspace_id, &model_name);
                                    count += 1;
                                }
                            }
                            if count > 0 {
                                info!(
                                    count,
                                    model = %model_name,
                                    "Synced embedding_model for workspaces"
                                );
                            }
                        }
                    }
                }
                Ok((Some(provider), None)) => {
                    // create_embedding_provider invariants say this should
                    // never happen. Success always produces a runtime
                    // status. Handle it defensively by publishing Ready
                    // with a synthesized status so the provider is still
                    // usable.
                    warn!(
                        "create_embedding_provider returned a provider without runtime status; \
                         publishing Ready with synthesized status"
                    );
                    let status = crate::embeddings::EmbeddingRuntimeStatus {
                        requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                        resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                        accelerated: false,
                        degraded_reason: Some(
                            "provider returned without runtime status (invariant violation)"
                                .to_string(),
                        ),
                    };
                    embedding_service_for_init.publish_ready(Arc::clone(&provider), status);
                    // Propagate to warmup-window watchers (see (Some, Some) arm comment).
                    watcher_pool_for_init
                        .update_all_provider(Some(provider))
                        .await;
                }
                Ok((None, status)) => {
                    // Provider failed to initialize or was intentionally
                    // disabled (e.g. JULIE_EMBEDDING_PROVIDER=none). Status
                    // is Some on failure, None on explicit disable.
                    let reason = status
                        .as_ref()
                        .and_then(|s| s.degraded_reason.clone())
                        .unwrap_or_else(|| {
                            "embedding provider disabled or failed to initialize".to_string()
                        });
                    embedding_service_for_init.publish_unavailable(reason, status);
                }
                Err(join_err) => {
                    // The spawn_blocking task panicked or was cancelled.
                    // CRITICAL: publish Unavailable so callers parked on
                    // wait_until_settled see the failure instead of hanging
                    // until their timeout elapses.
                    warn!(
                        error = ?join_err,
                        "Background embedding init task panicked or was cancelled; publishing Unavailable"
                    );
                    embedding_service_for_init.publish_unavailable(
                        format!("init task panicked/cancelled: {}", join_err),
                        None,
                    );
                }
            }
        });
    }

    let restart_notify = lifecycle.restart_notify();
    let (result, shutdown_cause) = tokio::select! {
        res = shutdown_signal() => {
            if let Err(e) = res {
                warn!("Signal handler setup failed: {}", e);
            }
            info!("Shutdown signal received, stopping daemon");
            (Ok(()), ShutdownCause::Signal)
        }
        _ = restart_notify.notified() => {
            info!("Stale binary restart triggered, stopping daemon");
            (Ok(()), ShutdownCause::RestartRequired)
        }
        _ = stop_notify.notified() => {
            info!("Shutdown event received from `julie stop`, stopping daemon");
            (Ok(()), ShutdownCause::StopCommand)
        }
    };

    // Give active sessions time to finish before tearing down shared resources.
    // Without this drain, sessions that are mid-request get dropped immediately
    // on SIGTERM, which can corrupt in-flight writes or leave daemon.db in an
    // inconsistent state.
    let remaining = sessions.active_count();
    let phase = lifecycle.request_shutdown(shutdown_cause, remaining);
    if remaining > 0 {
        let timeout = drain_timeout();
        info!(
            active_sessions = remaining,
            timeout_secs = timeout.as_secs(),
            "Draining active sessions"
        );
        let drained = drain_sessions(&sessions, timeout).await;
        if drained {
            info!("All sessions drained cleanly");
        } else {
            error!(
                remaining = sessions.active_count(),
                "Session drain timeout exceeded, forcing shutdown — in-flight writes may be lost"
            );
        }
    }
    if matches!(phase, LifecyclePhase::Draining { .. }) {
        lifecycle.sessions_drained();
    }

    info!(
        active_sessions = sessions.active_count(),
        "Daemon shutting down"
    );

    // Abort background tasks before tearing down shared resources.
    // These tasks hold Arcs into the pools; aborting them first prevents
    // them from racing with the explicit shutdown calls below.
    reaper_handle.abort();
    if let Some(cleanup_sweep_handle) = cleanup_sweep_handle {
        cleanup_sweep_handle.abort();
    }

    // LIFO shutdown: HTTP transport first (stops new requests from reaching
    // any service), then embedding service, then pools in dependency order
    // (workspace pool commits Tantivy writes before watcher pool releases OS
    // file-watcher handles).
    let port_path = paths.daemon_port();
    let artifacts = ShutdownArtifacts {
        port_path: &port_path,
        pid_file,
        state_path: &daemon_state_path,
    };
    perform_shutdown_sequence(
        http_transport,
        embedding_service,
        pool,
        watcher_pool,
        artifacts,
        None,
    )
    .await;

    info!("Daemon stopped");
    result
}

/// Files and resources cleaned up at the end of the shutdown sequence.
pub(crate) struct ShutdownArtifacts<'a> {
    pub port_path: &'a Path,
    pub pid_file: PidFile,
    pub state_path: &'a Path,
}

/// Execute the daemon shutdown sequence in LIFO dependency order.
///
/// Shutdown order:
///   1. HTTP transport — stops accepting new requests and drains existing ones.
///      Must happen first so in-flight requests cannot observe a torn-down
///      embedding service.
///   2. Embedding service — sidecar exit is awaited; safe now that the
///      transport gate is closed.
///   3. WorkspacePool — commits Tantivy writes and releases file locks.
///   4. WatcherPool — drops OS file-watcher handles; goes last because it
///      has the fewest downstream dependencies.
///   5. Housekeeping — removes port file, pid file, and daemon state file.
///
/// The `call_log` parameter is a test hook: when `Some`, each step records its
/// name to the log before executing. Pass `None` in production (zero overhead).
pub(crate) async fn perform_shutdown_sequence(
    http_transport: HttpTransportServer,
    embedding_service: Arc<EmbeddingService>,
    workspace_pool: Arc<WorkspacePool>,
    watcher_pool: Arc<WatcherPool>,
    artifacts: ShutdownArtifacts<'_>,
    call_log: Option<Arc<Mutex<Vec<&'static str>>>>,
) {
    // Helper: record a step name to the call_log if one was provided.
    let record = |step: &'static str| {
        let Some(log) = call_log.as_ref() else {
            return;
        };
        let Ok(mut guard) = log.lock() else {
            return;
        };
        guard.push(step);
    };

    // Step 1: HTTP transport
    record("http_transport");
    if let Err(e) = http_transport.shutdown().await {
        warn!("Failed to shut down HTTP MCP transport cleanly: {e:#}");
    }
    info!("HTTP MCP transport shut down");

    // Step 2: Embedding service
    record("embedding_service");
    embedding_service.shutdown().await;
    info!("Embedding service shut down");

    // Step 3: WorkspacePool (Tantivy writes committed, file locks released)
    record("workspace_pool");
    workspace_pool.shutdown().await;
    info!("Workspace pool shut down");

    // Step 4: WatcherPool (OS file-watcher handles released)
    record("watcher_pool");
    watcher_pool.shutdown().await;
    info!("Watcher pool shut down");

    // Step 5: Housekeeping
    let _ = std::fs::remove_file(artifacts.port_path);
    if let Err(e) = artifacts.pid_file.cleanup() {
        warn!("Failed to clean up PID file: {}", e);
    }
    let _ = std::fs::remove_file(artifacts.state_path);
}

/// Wait for a shutdown signal (SIGTERM or SIGINT on Unix).
async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm =
            signal(SignalKind::terminate()).context("failed to register SIGTERM handler")?;
        let mut sigint =
            signal(SignalKind::interrupt()).context("failed to register SIGINT handler")?;

        tokio::select! {
            _ = sigterm.recv() => info!("Received SIGTERM"),
            _ = sigint.recv() => info!("Received SIGINT"),
        }
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for ctrl-c")?;
        info!("Received Ctrl+C");
    }

    Ok(())
}
