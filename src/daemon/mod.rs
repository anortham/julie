//! Julie daemon: persistent background process serving MCP sessions.
//!
//! The canonical adapter path is Streamable HTTP over localhost.

pub mod app;
pub mod cli;

pub mod database;

pub mod discovery;
pub mod embedding_service;
pub mod http_client;
pub mod http_transport;
pub mod legacy_migration;
pub mod lifecycle;
pub mod mcp_session;
pub mod pid;
pub mod project_log;
pub mod singleton;
pub mod token_file;
pub mod session;
pub mod shutdown;
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
use tracing::{info, warn};

use crate::paths::DaemonPaths;
use crate::workspace::registry::generate_workspace_id;

pub use self::app::{DaemonApp, DaemonConfig, DaemonHandle, DaemonRuntimeContext};

use self::database::DaemonDatabase;
use self::embedding_service::EmbeddingService;
use self::http_transport::HttpTransportServer;
use self::pid::PidFile;
use self::session::SessionTracker;
use self::watcher_pool::WatcherPool;
use self::workspace_pool::WorkspacePool;

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
const DEFAULT_DRAIN_TIMEOUT_SECS: u64 = 60;
const MIN_DRAIN_TIMEOUT_SECS: u64 = 1;
const MAX_DRAIN_TIMEOUT_SECS: u64 = 120;

/// Return the configured drain timeout for the daemon shutdown sequence.
///
/// Reads `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` from the environment. Valid range
/// is [1, 120] seconds. Values outside this range, or unparseable strings,
/// fall back to the default (60 s) with a warning. The default is intentionally
/// generous so adapter-issued stops over slow filesystems (Windows NTFS fsync,
/// network mounts) finish in-flight work rather than abort it.
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
pub(crate) fn binary_mtime() -> Option<SystemTime> {
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
pub(crate) fn backfill_all_vector_counts(daemon_db: &DaemonDatabase, indexes_dir: &Path) {
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
pub(crate) fn migrate_stale_workspace_ids(daemon_db: &DaemonDatabase, indexes_dir: &Path) {
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
/// Run the Julie daemon: bind HTTP transport, accept connections, serve MCP.
///
/// Thin wrapper around `DaemonApp::new` + `serve`. Blocks until a shutdown
/// signal (SIGTERM/SIGINT on POSIX, ctrl_c on Windows) is received. HTTP is
/// the daemon MCP transport.
///
/// Retained as the public entry point so existing callers (`julie daemon`,
/// `julie-server daemon`, the integration test suite) keep working without
/// modification. New callers should construct a `DaemonApp` directly and
/// drive `serve`/`shutdown` themselves.
pub async fn run_daemon(paths: DaemonPaths, port: u16, no_dashboard: bool) -> Result<()> {
    paths
        .ensure_dirs()
        .context("Failed to create daemon directories")?;

    // Port fallback: try requested port, fall back to auto-assign on
    // EADDRINUSE. The listener we hand to DaemonApp::serve is the MCP HTTP
    // listener; the dashboard binds its own port internally.
    let listener = self::app::bind_mcp_listener_with_fallback(port).await?;
    let actual_port = listener.local_addr()?.port();

    let config = DaemonConfig {
        paths,
        port: actual_port,
        no_dashboard,
        runtime: DaemonRuntimeContext::default(),
    };

    let handle = DaemonApp::new(config)?.serve(listener).await?;

    // Block on shutdown signal. The named-event waker for `julie stop` /
    // `julie restart` on Windows lives inside `DaemonApp::serve` and triggers
    // shutdown by the same handle.shutdown() path on macOS/Linux.
    if let Err(e) = self::app::shutdown_signal().await {
        warn!("Signal handler setup failed: {}", e);
    }
    info!("Shutdown signal received, stopping daemon");

    handle.shutdown().await
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


