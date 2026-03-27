//! Julie daemon: persistent background process serving MCP over IPC.
//!
//! The daemon multiplexes many adapter sessions over IPC (Unix socket or Windows named pipe).
//! Each connection sends a `WORKSPACE:/path\n` header, then speaks MCP
//! JSON-RPC over the remaining stream.

pub mod database;
pub mod embedding_service;
pub mod ipc;
pub mod lifecycle;
pub mod pid;
pub mod project_log;
pub mod session;
pub mod watcher_pool;
pub mod workspace_pool;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
#[cfg(unix)]
use libc;
use rmcp::ServiceExt;
use tokio::io::AsyncReadExt;
use tokio::sync::broadcast;
use tokio::sync::Notify;
use tracing::{error, info, warn};

use crate::dashboard::state::DashboardEvent;

use crate::handler::JulieServerHandler;
use crate::paths::DaemonPaths;
use crate::workspace::registry::generate_workspace_id;

use self::database::DaemonDatabase;
use self::embedding_service::EmbeddingService;
use self::ipc::{IpcListener, IpcStream};
use self::pid::PidFile;
use self::session::SessionTracker;
use self::watcher_pool::WatcherPool;
use self::workspace_pool::WorkspacePool;

/// Classify an `accept()` error as transient or fatal.
///
/// Transient errors — connection resets, interrupts, and fd-exhaustion — should
/// be logged and retried. Fatal errors (unexpected listener state) should stop
/// the accept loop.
pub(crate) fn is_transient_accept_error(e: &io::Error) -> bool {
    match e.kind() {
        // Client vanished before the accept completed, or EINTR hit the syscall
        io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::Interrupted => true,
        _ => {
            if let Some(raw) = e.raw_os_error() {
                // EMFILE / ENFILE: per-process or system-wide fd exhaustion (Unix)
                #[cfg(unix)]
                if raw == libc::EMFILE || raw == libc::ENFILE {
                    return true;
                }
                // WSAEMFILE (10024): too many open sockets (Windows)
                #[cfg(windows)]
                if raw == 10024 {
                    return true;
                }
            }
            false
        }
    }
}

/// Wait for all active IPC sessions to finish, with a deadline.
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

/// Get the current on-disk binary's modification time.
///
/// Used at daemon startup to snapshot the binary mtime, then compared on each
/// session disconnect to detect whether the binary has been rebuilt. If it has,
/// the daemon exits after the last session disconnects so the adapter can
/// restart it with the new binary.
fn binary_mtime() -> Option<SystemTime> {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}

/// Reconcile workspace IDs after a normalize_path behavior change.
///
/// Compares each workspace's stored ID against the current generate_workspace_id
/// output. If they differ, renames the index directory and batch-updates the DB.
fn migrate_stale_workspace_ids(
    daemon_db: &DaemonDatabase,
    indexes_dir: &Path,
) {
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
                    old_id, new_id,
                    "Failed to rename index dir, skipping DB migration for this entry: {}", e
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
                info!(old_id, "Removed stale index directory (new dir already exists)");
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

/// Run the Julie daemon: bind IPC socket, accept connections, serve MCP.
///
/// This function blocks until a shutdown signal (SIGTERM/SIGINT) is received.
/// Each incoming IPC connection is handled in its own tokio task. The daemon
/// is workspace-agnostic; the workspace path arrives per-session via the
/// IPC header protocol.
pub async fn run_daemon(paths: DaemonPaths, port: u16, no_dashboard: bool) -> Result<()> {
    paths
        .ensure_dirs()
        .context("Failed to create daemon directories")?;

    // Atomically check-and-create the PID file. create_exclusive uses O_CREAT|O_EXCL
    // internally, eliminating the TOCTOU window between check_running and create
    // that allowed two concurrent invocations to both believe they were first.
    let pid_file =
        PidFile::create_exclusive(&paths.daemon_pid()).context("Failed to start daemon")?;
    info!(pid = std::process::id(), "Daemon PID file created");

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
    }

    // Initialize shared embedding service (blocking: model load / sidecar bootstrap)
    let embedding_service = Arc::new(
        tokio::task::spawn_blocking(|| EmbeddingService::initialize())
            .await
            .context("Embedding service initialization panicked")?,
    );
    info!(
        available = embedding_service.is_available(),
        "Shared embedding service initialized"
    );

    // Capture binary mtime at startup for stale-binary detection.
    // If the binary is rebuilt while the daemon is running, the next session
    // disconnect will detect the mismatch and trigger a graceful restart.
    let startup_binary_mtime = binary_mtime();
    let restart_pending = Arc::new(AtomicBool::new(false));
    if startup_binary_mtime.is_some() {
        info!("Binary mtime captured for stale-binary detection");
    } else {
        warn!("Could not determine binary mtime; stale-binary detection disabled");
    }

    // Shared state
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let reaper_handle = watcher_pool.spawn_reaper(Duration::from_secs(60));
    info!("WatcherPool started (grace=300s, reaper=60s)");

    let pool = Arc::new(WorkspacePool::new(
        paths.indexes_dir(),
        daemon_db.clone(),
        Some(watcher_pool),
        Some(Arc::clone(&embedding_service)),
    ));
    let sessions = Arc::new(SessionTracker::new());

    // Notify used by the accept loop to trigger graceful shutdown when the
    // last session disconnects and the binary is stale. This replaces the
    // Unix-only SIGTERM-to-self pattern with a cross-platform mechanism that
    // feeds into the same cleanup path below.
    let restart_notify = Arc::new(Notify::new());

    // --- Dashboard HTTP server ---
    let dashboard_state = crate::dashboard::state::DashboardState::new(
        Arc::clone(&sessions),
        daemon_db.clone(),
        Arc::clone(&restart_pending),
        std::time::Instant::now(),
        embedding_service.is_available(),
        Some(Arc::clone(&pool)),
        50, // error buffer capacity
    );

    // Extract the broadcast sender before dashboard_state is moved into the router.
    // Cloned cheaply (Arc-backed) and passed to each IPC session for live-feed events.
    let dashboard_tx: broadcast::Sender<DashboardEvent> = dashboard_state.sender();

    let dashboard_config = crate::dashboard::DashboardConfig::default();
    let dashboard_router = crate::dashboard::create_router(dashboard_state, dashboard_config)
        .context("Failed to initialize dashboard templates")?;

    // Try requested port, fall back to auto-assign
    let http_listener = match tokio::net::TcpListener::bind(
        format!("127.0.0.1:{}", port),
    )
    .await
    {
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

    // Auto-open browser unless suppressed
    if !no_dashboard {
        if let Err(e) = opener::open(&dashboard_url) {
            warn!("Failed to open browser: {}", e);
        }
    }

    // Spawn HTTP server as background task
    tokio::spawn(async move {
        if let Err(e) = axum::serve(http_listener, dashboard_router).await {
            tracing::error!("Dashboard HTTP server error: {}", e);
        }
    });

    // Bind the IPC listener AFTER all initialization is complete. On Windows,
    // the adapter probes the named pipe to detect readiness, and that probe
    // consumes a pipe instance. If the pipe is bound before the accept loop
    // starts (e.g., during the 8+ second embedding model load), the probe eats
    // the only instance and the real connection gets ERROR_PIPE_BUSY (231).
    let listener = IpcListener::bind(&paths.daemon_ipc_addr())
        .await
        .context("Failed to bind IPC endpoint")?;

    info!(
        endpoint = %paths.daemon_ipc_addr().display(),
        "Daemon listening for IPC connections"
    );

    // Accept loop with graceful shutdown
    let result = tokio::select! {
        res = accept_loop(&listener, &pool, &sessions, &daemon_db, &embedding_service, startup_binary_mtime, &restart_pending, &restart_notify, dashboard_tx) => res,
        res = shutdown_signal() => {
            if let Err(e) = res {
                warn!("Signal handler setup failed: {}", e);
            }
            info!("Shutdown signal received, stopping daemon");
            Ok(())
        }
        _ = restart_notify.notified() => {
            info!("Stale binary restart triggered, stopping daemon");
            Ok(())
        }
    };

    // Give active sessions time to finish before tearing down shared resources.
    // Without this drain, sessions that are mid-request get dropped immediately
    // on SIGTERM, which can corrupt in-flight writes or leave daemon.db in an
    // inconsistent state.
    let remaining = sessions.active_count();
    if remaining > 0 {
        info!(
            active_sessions = remaining,
            "Draining active sessions (up to 5s)"
        );
        let drained = drain_sessions(&sessions, Duration::from_secs(5)).await;
        if drained {
            info!("All sessions drained cleanly");
        } else {
            warn!(
                remaining = sessions.active_count(),
                "Session drain timeout exceeded, forcing shutdown"
            );
        }
    }

    info!(
        active_sessions = sessions.active_count(),
        "Daemon shutting down"
    );

    reaper_handle.abort();

    embedding_service.shutdown();
    info!("Embedding service shut down");

    listener.cleanup();
    let _ = std::fs::remove_file(paths.daemon_port());

    if let Err(e) = pid_file.cleanup() {
        warn!("Failed to clean up PID file: {}", e);
    }

    info!("Daemon stopped");
    result
}

/// Accept IPC connections in a loop, spawning a task for each.
///
/// When the last session disconnects and the on-disk binary has been rebuilt
/// since this daemon started, the loop exits cleanly. The adapter will
/// auto-start a fresh daemon with the new binary on the next connection.
async fn accept_loop(
    listener: &IpcListener,
    pool: &Arc<WorkspacePool>,
    sessions: &Arc<SessionTracker>,
    daemon_db: &Option<Arc<DaemonDatabase>>,
    embedding_service: &Arc<EmbeddingService>,
    startup_binary_mtime: Option<SystemTime>,
    restart_pending: &Arc<AtomicBool>,
    restart_notify: &Arc<Notify>,
    dashboard_tx: broadcast::Sender<DashboardEvent>,
) -> Result<()> {
    loop {
        let stream = match listener.accept().await {
            Ok(s) => s,
            Err(e) if is_transient_accept_error(&e) => {
                // Transient OS error (connection reset, EINTR, fd exhaustion, etc.).
                // Log and retry — killing the daemon for EMFILE would be wrong.
                warn!(error = %e, "Transient IPC accept error, retrying");
                // Back off briefly on fd exhaustion to let pressure ease
                #[cfg(unix)]
                if let Some(raw) = e.raw_os_error() {
                    if raw == libc::EMFILE || raw == libc::ENFILE {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
                continue;
            }
            Err(e) => {
                error!(error = %e, "Fatal IPC accept error, stopping accept loop");
                return Err(anyhow::anyhow!("IPC accept failed: {}", e));
            }
        };

        let pool = Arc::clone(pool);
        let sessions = Arc::clone(sessions);
        let daemon_db = daemon_db.clone();
        let embedding_service = Arc::clone(embedding_service);
        let restart_pending = Arc::clone(restart_pending);
        let restart_notify = Arc::clone(restart_notify);
        let dashboard_tx = dashboard_tx.clone();
        let session_id = sessions.add_session();

        // Check for stale binary on each new connection. This way the health
        // check can surface it immediately rather than waiting for disconnect.
        if let Some(startup_mtime) = startup_binary_mtime {
            if let Some(current_mtime) = binary_mtime() {
                if current_mtime > startup_mtime && !restart_pending.load(Ordering::Relaxed) {
                    restart_pending.store(true, Ordering::Relaxed);
                    warn!(
                        "Binary has been rebuilt since daemon started. \
                         Daemon will restart when all sessions disconnect."
                    );
                }
            }
        }

        info!(
            session_id = %session_id,
            active = sessions.active_count(),
            "New IPC session accepted"
        );

        tokio::spawn(async move {
            if let Err(e) = handle_ipc_session(
                stream,
                &pool,
                &session_id,
                &daemon_db,
                &embedding_service,
                &restart_pending,
                Some(dashboard_tx),
            )
            .await
            {
                error!(session_id = %session_id, "IPC session error: {}", e);
            }

            sessions.remove_session(&session_id);
            let remaining = sessions.active_count();
            info!(
                session_id = %session_id,
                remaining,
                "IPC session ended"
            );

            // Check for stale binary at disconnect time too (not just on new
            // connections). Without this, a rebuild during an active session
            // is missed: the session disconnects, restart_pending is still
            // false, and a new session connects before the daemon can exit.
            if let Some(startup_mtime) = startup_binary_mtime {
                if let Some(current_mtime) = binary_mtime() {
                    if current_mtime > startup_mtime && !restart_pending.load(Ordering::Relaxed) {
                        restart_pending.store(true, Ordering::Relaxed);
                        warn!("Binary rebuild detected at session disconnect.");
                    }
                }
            }

            // If the binary has been rebuilt and this was the last session,
            // signal the daemon to exit. The adapter will auto-start a fresh
            // daemon with the new binary on the next connection.
            if remaining == 0 && restart_pending.load(Ordering::Relaxed) {
                info!("Last session disconnected and binary is stale. Triggering restart.");
                // Wake the select! in run_daemon so it exits through the
                // normal cleanup path (drain, embedding shutdown, PID cleanup).
                restart_notify.notify_one();
            }
        });
    }
}

/// Handle a single IPC session: read the workspace header, then serve MCP.
async fn handle_ipc_session(
    mut stream: IpcStream,
    pool: &WorkspacePool,
    session_id: &str,
    daemon_db: &Option<Arc<DaemonDatabase>>,
    embedding_service: &Arc<EmbeddingService>,
    restart_pending: &Arc<AtomicBool>,
    dashboard_tx: Option<broadcast::Sender<DashboardEvent>>,
) -> Result<()> {
    // Read the workspace header with a timeout so a misbehaving client that
    // connects but never sends the header cannot hold a session slot forever.
    let workspace_path = tokio::time::timeout(
        Duration::from_secs(5),
        read_workspace_header(&mut stream),
    )
    .await
    .context("Workspace header read timed out (5s)")?
    .context("Failed to read workspace header")?;

    info!(
        session_id = %session_id,
        workspace = %workspace_path.display(),
        "Session workspace resolved"
    );

    // Compute workspace ID from path. Use generate_workspace_id() directly
    // (produces e.g. "julie_316c0b08"). Do NOT wrap in another prefix; the
    // indexing pipeline also calls generate_workspace_id() and the IDs must match
    // for daemon.db FK constraints and workspace_db_path() to resolve correctly.
    let path_str = workspace_path.to_string_lossy().to_string();
    let full_workspace_id =
        generate_workspace_id(&path_str).context("Failed to generate workspace ID")?;

    info!(
        session_id = %session_id,
        workspace_id = %full_workspace_id,
        "Getting or initializing workspace from pool"
    );

    // Get or create shared workspace from the pool
    let workspace = pool
        .get_or_init(&full_workspace_id, workspace_path.clone())
        .await
        .context("Failed to initialize workspace in pool")?;

    // From this point, disconnect_session must run on all exit paths — even
    // on errors from handler creation or MCP serving. Wrap the session work in
    // an async block so `?` propagates to the block result rather than the
    // outer function, allowing cleanup to always execute afterwards.
    let session_result: Result<()> = async {
        // Create a per-session handler backed by the shared workspace
        let handler = JulieServerHandler::new_with_shared_workspace(
            workspace,
            workspace_path,
            daemon_db.clone(),
            Some(full_workspace_id.clone()),
            Some(Arc::clone(embedding_service)),
            Some(Arc::clone(restart_pending)),
            dashboard_tx,
        )
        .await
        .context("Failed to create handler for IPC session")?;

        // Auto-attach reference workspaces registered for this primary workspace.
        // Each reference is pre-loaded into the pool so its indexes are warm.
        if let Some(db) = &daemon_db {
            match db.list_references(&full_workspace_id) {
                Ok(refs) => {
                    for ref_ws in &refs {
                        match pool
                            .get_or_init(&ref_ws.workspace_id, PathBuf::from(&ref_ws.path))
                            .await
                        {
                            Ok(_) => {
                                info!(
                                    session_id = %session_id,
                                    reference = %ref_ws.workspace_id,
                                    "Auto-attached reference workspace"
                                );
                            }
                            Err(e) => {
                                warn!(
                                    session_id = %session_id,
                                    reference = %ref_ws.workspace_id,
                                    "Failed to auto-attach reference workspace: {}", e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        session_id = %session_id,
                        "Failed to query reference workspaces: {}", e
                    );
                }
            }
        }

        // Grab project log before serve() consumes the handler
        let project_log = handler.project_log.clone();

        // Log session start to project log
        if let Some(ref log) = project_log {
            log.session_start(session_id);
        }

        // Serve MCP over the IPC stream. IpcStream implements AsyncRead + AsyncWrite,
        // so rmcp's blanket IntoTransport impl handles the conversion automatically.
        let service = handler
            .serve(stream)
            .await
            .map_err(|e| anyhow::anyhow!("MCP serve failed: {}", e))?;

        // Block until the MCP session ends (client disconnect or error)
        let result = match service.waiting().await {
            Ok(_reason) => {
                info!(session_id = %session_id, "MCP session completed normally");
                Ok(())
            }
            Err(e) => {
                warn!(session_id = %session_id, "MCP session ended with error: {}", e);
                Err(anyhow::anyhow!("MCP session error: {}", e))
            }
        };

        // Log session end to project log
        if let Some(ref log) = project_log {
            log.session_end(session_id);
        }

        result
    }
    .await;

    // Sync the pool's in-memory `indexed` flag from daemon.db. If indexing ran
    // during this session, handle_index_command will have already written "ready"
    // to daemon.db; propagate that status to the pool so is_indexed() returns true
    // for subsequent sessions without requiring another indexing pass.
    pool.sync_indexed_from_db(&full_workspace_id).await;

    // Decrement session count in daemon.db (pool handles the None case gracefully)
    pool.disconnect_session(&full_workspace_id).await;

    session_result
}

/// Read the workspace header from an IPC stream.
///
/// The adapter sends a single line: `WORKSPACE:/path/to/project\n`
/// We read byte-by-byte to avoid BufReader consuming bytes past the newline,
/// which would break the subsequent MCP JSON-RPC framing.
async fn read_workspace_header(stream: &mut IpcStream) -> Result<PathBuf> {
    let mut header = Vec::new();
    let mut buf = [0u8; 1];

    loop {
        stream
            .read_exact(&mut buf)
            .await
            .context("Failed to read workspace header")?;
        if buf[0] == b'\n' {
            break;
        }
        header.push(buf[0]);

        // Safety limit: workspace paths shouldn't exceed 4 KB
        if header.len() > 4096 {
            anyhow::bail!("Workspace header too long (>4096 bytes)");
        }
    }

    let header = String::from_utf8(header).context("Workspace header is not valid UTF-8")?;

    let path = header.strip_prefix("WORKSPACE:").ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid IPC header: expected WORKSPACE:<path>, got: {}",
            header
        )
    })?;

    Ok(PathBuf::from(path))
}

/// Wait for a shutdown signal (SIGTERM or SIGINT on Unix).
async fn shutdown_signal() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate())
            .context("failed to register SIGTERM handler")?;
        let mut sigint = signal(SignalKind::interrupt())
            .context("failed to register SIGINT handler")?;

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
