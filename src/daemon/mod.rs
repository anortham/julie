//! Julie daemon: persistent background process serving MCP over IPC.
//!
//! The daemon multiplexes many adapter sessions over a single Unix socket.
//! Each connection sends a `WORKSPACE:/path\n` header, then speaks MCP
//! JSON-RPC over the remaining stream.

pub mod database;
pub mod ipc;
pub mod lifecycle;
pub mod pid;
pub mod project_log;
pub mod session;
pub mod watcher_pool;
pub mod workspace_pool;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rmcp::ServiceExt;
use tokio::io::AsyncReadExt;
use tracing::{error, info, warn};

use crate::handler::JulieServerHandler;
use crate::paths::DaemonPaths;
use crate::workspace::registry::generate_workspace_id;

use self::database::DaemonDatabase;
use self::ipc::IpcListener;
use self::pid::PidFile;
use self::session::SessionTracker;
use self::watcher_pool::WatcherPool;
use self::workspace_pool::WorkspacePool;

/// Run the Julie daemon: bind IPC socket, accept connections, serve MCP.
///
/// This function blocks until a shutdown signal (SIGTERM/SIGINT) is received.
/// Each incoming IPC connection is handled in its own tokio task. The daemon
/// is workspace-agnostic; the workspace path arrives per-session via the
/// IPC header protocol.
pub async fn run_daemon(paths: DaemonPaths, _port: u16) -> Result<()> {
    paths
        .ensure_dirs()
        .context("Failed to create daemon directories")?;

    // Check for an already-running daemon
    if let Some(pid) = PidFile::check_running(&paths.daemon_pid()) {
        anyhow::bail!("Daemon already running (PID {})", pid);
    }

    // Write our PID file
    let pid_file = PidFile::create(&paths.daemon_pid())
        .context("Failed to create PID file")?;
    info!(pid = std::process::id(), "Daemon PID file created");

    // Bind the IPC listener (Unix socket)
    #[cfg(unix)]
    let listener = IpcListener::bind(&paths.daemon_socket())
        .await
        .context("Failed to bind IPC socket")?;

    info!(
        socket = %paths.daemon_socket().display(),
        "Daemon listening for IPC connections"
    );

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
            warn!("Failed to open daemon.db, continuing without persistence: {}", e);
            None
        }
    };

    // Shared state
    let watcher_pool = Arc::new(WatcherPool::new(Duration::from_secs(300)));
    let _reaper = watcher_pool.spawn_reaper(Duration::from_secs(60));
    info!("WatcherPool started (grace=300s, reaper=60s)");

    let pool = Arc::new(WorkspacePool::new(
        paths.indexes_dir(),
        daemon_db.clone(),
        Some(watcher_pool),
    ));
    let sessions = Arc::new(SessionTracker::new());

    // Accept loop with graceful shutdown
    let result = tokio::select! {
        res = accept_loop(&listener, &pool, &sessions, &daemon_db) => res,
        _ = shutdown_signal() => {
            info!("Shutdown signal received, stopping daemon");
            Ok(())
        }
    };

    // Cleanup
    info!(
        active_sessions = sessions.active_count(),
        "Daemon shutting down"
    );

    #[cfg(unix)]
    listener.cleanup();

    if let Err(e) = pid_file.cleanup() {
        warn!("Failed to clean up PID file: {}", e);
    }

    info!("Daemon stopped");
    result
}

/// Accept IPC connections in a loop, spawning a task for each.
async fn accept_loop(
    listener: &IpcListener,
    pool: &Arc<WorkspacePool>,
    sessions: &Arc<SessionTracker>,
    daemon_db: &Option<Arc<DaemonDatabase>>,
) -> Result<()> {
    loop {
        let stream = listener.accept().await.context("IPC accept failed")?;

        let pool = Arc::clone(pool);
        let sessions = Arc::clone(sessions);
        let daemon_db = daemon_db.clone();
        let session_id = sessions.add_session();

        info!(
            session_id = %session_id,
            active = sessions.active_count(),
            "New IPC session accepted"
        );

        tokio::spawn(async move {
            if let Err(e) = handle_ipc_session(stream, &pool, &session_id, &daemon_db).await {
                error!(session_id = %session_id, "IPC session error: {}", e);
            }

            sessions.remove_session(&session_id);
            info!(
                session_id = %session_id,
                remaining = sessions.active_count(),
                "IPC session ended"
            );
        });
    }
}

/// Handle a single IPC session: read the workspace header, then serve MCP.
async fn handle_ipc_session(
    mut stream: tokio::net::UnixStream,
    pool: &WorkspacePool,
    session_id: &str,
    daemon_db: &Option<Arc<DaemonDatabase>>,
) -> Result<()> {
    // Read the workspace header (byte-by-byte to avoid BufReader buffering issues)
    let workspace_path = read_workspace_header(&mut stream).await?;

    info!(
        session_id = %session_id,
        workspace = %workspace_path.display(),
        "Session workspace resolved"
    );

    // Compute workspace ID from path
    let path_str = workspace_path.to_string_lossy().to_string();
    let workspace_id = generate_workspace_id(&path_str)
        .context("Failed to generate workspace ID")?;

    // Prefix with directory name for readability (same pattern as handler.rs)
    let dir_name = workspace_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let full_workspace_id = format!("{}_{}", dir_name, &workspace_id[..8.min(workspace_id.len())]);

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

    // Create a per-session handler backed by the shared workspace
    let handler = JulieServerHandler::new_with_shared_workspace(
        workspace,
        workspace_path,
        daemon_db.clone(),
        Some(full_workspace_id.clone()),
    )
    .await
    .context("Failed to create handler for IPC session")?;

    // Auto-attach reference workspaces registered for this primary workspace.
    // Each reference is pre-loaded into the pool so its indexes are warm.
    if let Some(db) = &daemon_db {
        match db.list_references(&full_workspace_id) {
            Ok(refs) => {
                for ref_ws in &refs {
                    match pool.get_or_init(&ref_ws.workspace_id, PathBuf::from(&ref_ws.path)).await {
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

    // Serve MCP over the IPC stream. UnixStream implements AsyncRead + AsyncWrite,
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

    // Decrement session count in daemon.db (pool handles the None case gracefully)
    pool.disconnect_session(&full_workspace_id).await;

    result
}

/// Read the workspace header from an IPC stream.
///
/// The adapter sends a single line: `WORKSPACE:/path/to/project\n`
/// We read byte-by-byte to avoid BufReader consuming bytes past the newline,
/// which would break the subsequent MCP JSON-RPC framing.
async fn read_workspace_header(stream: &mut tokio::net::UnixStream) -> Result<PathBuf> {
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

    let header = String::from_utf8(header)
        .context("Workspace header is not valid UTF-8")?;

    let path = header
        .strip_prefix("WORKSPACE:")
        .ok_or_else(|| anyhow::anyhow!("Invalid IPC header: expected WORKSPACE:<path>, got: {}", header))?;

    Ok(PathBuf::from(path))
}

/// Wait for a shutdown signal (SIGTERM or SIGINT on Unix).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM");
        let mut sigint = signal(SignalKind::interrupt()).expect("failed to register SIGINT");

        tokio::select! {
            _ = sigterm.recv() => info!("Received SIGTERM"),
            _ = sigint.recv() => info!("Received SIGINT"),
        }
    }

    #[cfg(windows)]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
        info!("Received Ctrl+C");
    }
}
