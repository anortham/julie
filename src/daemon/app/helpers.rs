//! Free helpers that DaemonApp uses internally. Kept out of `app.rs` so the
//! public surface (DaemonApp, DaemonConfig, DaemonHandle, DaemonRuntimeContext)
//! stays under the 500-line file budget.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::daemon::database::DaemonDatabase;
use crate::daemon::discovery::{AcquireError, DaemonLockGuard};
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::daemon::workspace_registry_store::WorkspaceRegistryStore;
use crate::daemon::{backfill_all_vector_counts, migrate_stale_workspace_ids};
use crate::paths::DaemonPaths;

/// Acquire the daemon singleton lock, yielding to any existing daemon.
///
/// This is the FIRST gate in `run_daemon` startup. Acquiring the lock here
/// (before binding any listening sockets) collapses the previous startup
/// thundering herd where N concurrent `julie-server daemon` invocations
/// would each bind a port (falling back to auto-assigned) and run partial
/// init before the in-app lock check killed all but one.
///
/// Returns `Ok(Some(guard))` if this process is the unique daemon for the
/// JULIE_HOME. Caller holds the guard for the daemon's lifetime; the
/// kernel releases the lock on process exit (clean or crash).
///
/// Returns `Ok(None)` if another daemon is already running. Caller should
/// exit silently with status 0 — this is the expected outcome when the
/// adapter spawns a daemon and one is already up.
///
/// Returns `Err` only on real I/O failures (permission denied, parent dir
/// missing, etc).
pub(crate) fn acquire_or_yield_to_existing_daemon(
    paths: &DaemonPaths,
) -> Result<Option<DaemonLockGuard>> {
    match DaemonLockGuard::try_acquire(&paths.daemon_lock()) {
        Ok(guard) => Ok(Some(guard)),
        Err(AcquireError::AlreadyHeld(_)) => Ok(None),
        Err(other) => Err(anyhow::anyhow!("{}", other)).context("Failed to acquire daemon lock"),
    }
}

/// Open the persistent daemon database with crash recovery and run all
/// startup-time migrations (stale workspace IDs, normalized paths, vector
/// counts). Returns `None` when the DB can't be opened — the daemon then
/// runs without persistence.
///
/// Crash recovery: resets stale session counts from any previous run and
/// prunes tool call records older than 90 days.
pub(crate) fn open_and_migrate_daemon_db(paths: &DaemonPaths) -> Option<Arc<DaemonDatabase>> {
    let db = match DaemonDatabase::open(&paths.daemon_db()) {
        Ok(db) => db,
        Err(e) => {
            warn!(
                "Failed to open daemon.db, continuing without persistence: {}",
                e
            );
            return None;
        }
    };
    if let Err(e) = db.reset_all_session_counts() {
        warn!("Failed to reset session counts: {}", e);
    }
    if let Err(e) = db.prune_tool_calls(90) {
        warn!("Failed to prune old tool calls: {}", e);
    }
    info!("Daemon database ready: {}", paths.daemon_db().display());
    let db = Arc::new(db);

    // Migrate stale workspace IDs from pre-v6.0.4 normalize_path behavior.
    // Must run before WorkspacePool is created so sessions see correct IDs.
    migrate_stale_workspace_ids(&db, &paths.indexes_dir());

    // Normalize path separators (fixes adapter's previous forward-slash
    // storage) and restore "ready" status for workspaces stuck at "pending".
    match db.normalize_workspace_paths() {
        Ok(0) => {}
        Ok(n) => info!(count = n, "Normalized workspace paths in daemon.db"),
        Err(e) => warn!("Failed to normalize workspace paths: {}", e),
    }

    // Backfill vector_count for workspaces that have embeddings but no
    // count in daemon.db (handles workspaces embedded before this stat
    // was tracked).
    backfill_all_vector_counts(&db, &paths.indexes_dir());

    Some(db)
}

/// Wait for SIGTERM/SIGINT on Unix or Ctrl-C on Windows.
///
/// Lives at module scope so `run_daemon` (the thin wrapper in `daemon/mod.rs`)
/// can await it after handing the daemon body to `DaemonApp::serve`.
pub(crate) async fn shutdown_signal() -> Result<()> {
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

/// Bind the MCP HTTP listener with port fallback semantics.
///
/// Tries the requested `port` first; on `EADDRINUSE` falls back to an
/// auto-assigned port and logs a warning. `port == 0` means "let the kernel
/// choose"; we don't bother with the fallback path in that case.
pub(crate) async fn bind_mcp_listener_with_fallback(port: u16) -> Result<TcpListener> {
    match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
        Ok(l) => Ok(l),
        Err(_) if port != 0 => {
            warn!(
                "Port {} in use, falling back to auto-assigned MCP transport port",
                port
            );
            TcpListener::bind("127.0.0.1:0")
                .await
                .context("Failed to bind MCP HTTP transport on any port")
        }
        Err(e) => Err(anyhow::anyhow!("Failed to bind MCP HTTP transport: {}", e)),
    }
}

/// Spawn the background workspace-cleanup sweep task.
///
/// The sweep runs every 10 minutes; it prunes stale workspace registrations
/// and orphan index directories. Returns `None` when no daemon DB is
/// available (sweep needs it to enumerate workspaces).
pub(crate) fn spawn_cleanup_sweep(
    daemon_db: Option<&Arc<DaemonDatabase>>,
    indexes_dir: std::path::PathBuf,
    workspace_pool: Arc<WorkspacePool>,
    watcher_pool: Arc<WatcherPool>,
) -> Option<JoinHandle<()>> {
    let daemon_db = daemon_db.cloned()?;
    let registry_store = WorkspaceRegistryStore::new(daemon_db, indexes_dir);
    Some(tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tick.tick().await;
            let cleanup_activity =
                crate::tools::workspace::commands::registry::cleanup::WorkspaceCleanupActivity::new(
                    Some(&workspace_pool),
                    Some(&watcher_pool),
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
                Err(e) => warn!("Background workspace cleanup sweep failed: {}", e),
            }
        }
    }))
}

/// Set up the Windows named-event waker for `julie stop` / `julie restart`.
///
/// On Windows, `ctrl_c()` requires a console (which `CREATE_NO_WINDOW`
/// daemons lack), so this named event is the primary graceful-shutdown
/// mechanism. Returns the `Notify` the caller should `.notified()` on.
/// On Unix this is a no-op that just returns an unused `Notify`.
#[allow(unused_variables)]
pub(crate) fn setup_stop_notify(paths: &DaemonPaths) -> Arc<Notify> {
    let stop_notify = Arc::new(Notify::new());
    #[cfg(windows)]
    {
        let event_name = paths.daemon_shutdown_event();
        match crate::daemon::shutdown_event::ShutdownEvent::create(&event_name) {
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
    stop_notify
}

/// Bind the dashboard HTTP listener (auto-assigned port) and write the
/// resolved port to `daemon_port` so `julie dashboard` can find it.
///
/// Returns the bound listener and the chosen port.
pub(crate) async fn bind_dashboard_listener_and_publish(
    port_file: &Path,
) -> Result<(TcpListener, u16)> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("Failed to bind dashboard HTTP server")?;
    let port = listener.local_addr()?.port();
    std::fs::write(port_file, port.to_string()).context("Failed to write daemon port file")?;
    Ok((listener, port))
}

/// Spawn the background task that bootstraps the embedding provider and
/// publishes it on `EmbeddingService`. Returns a JoinHandle so the daemon can
/// abort it during shutdown.
///
/// The `Err(join_err)` arm is critical: without it, a panicking init task
/// would leave the service stuck in `Initializing` forever and every future
/// `wait_until_settled` would time out rather than report the real failure.
/// See Task 2 of docs/plans/2026-04-09-daemon-lazy-embedding-init.md.
pub(crate) fn spawn_embedding_init(
    embedding_service: Arc<EmbeddingService>,
    daemon_db: Option<Arc<DaemonDatabase>>,
    watcher_pool: Arc<WatcherPool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!("Background embedding init task started");
        let init_result =
            tokio::task::spawn_blocking(|| crate::embeddings::create_embedding_provider()).await;

        match init_result {
            Ok((Some(provider), Some(status))) => {
                let model_name = provider.device_info().model_name.clone();
                embedding_service.publish_ready(Arc::clone(&provider), status);

                // Propagate the provider to any watchers attached during the
                // warmup window. They start with None in their
                // SharedEmbeddingProvider RwLock cell (because
                // shared_embedding_provider() returned None while the service
                // was Initializing), and would never see the new provider
                // without this push. Watchers attached AFTER publish_ready
                // get the provider via their normal attach path, so this
                // only matters for the warmup race.
                watcher_pool
                    .update_all_provider(Some(Arc::clone(&provider)))
                    .await;

                // Sync embedding_model for workspaces that have vectors but
                // a missing or stale model name. Previously ran on the
                // critical path right after EmbeddingService::initialize;
                // now it runs here, once the background init actually
                // produces a provider.
                if let Some(ref db) = daemon_db {
                    if let Ok(workspaces) = db.list_workspaces() {
                        let mut count = 0;
                        for ws in &workspaces {
                            if ws.vector_count.map_or(false, |v| v > 0)
                                && ws.embedding_model.as_deref() != Some(model_name.as_str())
                            {
                                let _ = db.update_embedding_model(&ws.workspace_id, &model_name);
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
                // create_embedding_provider invariants say this should never
                // happen. Success always produces a runtime status. Handle
                // defensively by publishing Ready with a synthesized status.
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
                embedding_service.publish_ready(Arc::clone(&provider), status);
                watcher_pool.update_all_provider(Some(provider)).await;
            }
            Ok((None, status)) => {
                // Provider failed to initialize or was intentionally disabled
                // (e.g. JULIE_EMBEDDING_PROVIDER=none). Status is Some on
                // failure, None on explicit disable.
                let reason = status
                    .as_ref()
                    .and_then(|s| s.degraded_reason.clone())
                    .unwrap_or_else(|| {
                        "embedding provider disabled or failed to initialize".to_string()
                    });
                embedding_service.publish_unavailable(reason, status);
            }
            Err(join_err) => {
                // CRITICAL: publish Unavailable so callers parked on
                // wait_until_settled see the failure instead of hanging
                // until their timeout elapses.
                warn!(
                    error = ?join_err,
                    "Background embedding init task panicked or was cancelled; publishing Unavailable"
                );
                embedding_service.publish_unavailable(
                    format!("init task panicked/cancelled: {}", join_err),
                    None,
                );
            }
        }
    })
}
