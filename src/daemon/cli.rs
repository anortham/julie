//! CLI entry point for the `julie-daemon` binary.
//!
//! Parses `start | stop | status` from argv and dispatches to the appropriate
//! daemon lifecycle function.  All three subcommands delegate to existing code
//! paths so behavior is identical to `julie-server daemon`, `julie-server stop`,
//! and `julie-server status` today.
//!
//! A1.8: The lifecycle bodies are factored into `start_daemon`, `stop_daemon`,
//! and `status_daemon` free functions so the compatibility shim in
//! `src/main.rs` (legacy `julie-server`) can dispatch through the same code
//! without duplicating logging setup, legacy migration gates, or recovery
//! marker reporting.

use std::fs;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_appender::non_blocking;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::paths::DaemonPaths;

// ---------------------------------------------------------------------------
// CLI shape
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "julie-daemon",
    version,
    about = "Julie daemon lifecycle control"
)]
struct DaemonCli {
    #[command(subcommand)]
    command: DaemonCommand,
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Start the daemon (HTTP + IPC transport)
    Start {
        /// MCP HTTP port for the daemon (default: 7890, fallback to auto if taken). Dashboard auto-assigns its own port.
        #[arg(long, default_value = "7890")]
        port: u16,
        /// Disable auto-opening dashboard in browser
        #[arg(long)]
        no_dashboard: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Check daemon status
    Status,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Parse argv and dispatch to the appropriate lifecycle function.
///
/// Called by `src/bin/julie-daemon.rs`.
pub async fn run() -> Result<()> {
    let cli = DaemonCli::parse();
    let paths = DaemonPaths::new();

    match cli.command {
        DaemonCommand::Start { port, no_dashboard } => start_daemon(paths, port, no_dashboard).await,
        DaemonCommand::Stop => stop_daemon(&paths),
        DaemonCommand::Status => {
            status_daemon(&paths);
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Lifecycle helpers (shared with the `julie-server` compat shim, A1.8)
// ---------------------------------------------------------------------------

/// Start the daemon with the legacy-migration gate, logging setup, and
/// blocking `run_daemon` loop. Returns when the daemon exits.
///
/// Shared between `julie-daemon start` and `julie-server daemon`.
pub async fn start_daemon(paths: DaemonPaths, port: u16, no_dashboard: bool) -> Result<()> {
    // A1.5: hard legacy-migration gate. Refuse to start if a legacy
    // julie-server daemon is running for the same JULIE_HOME — both
    // would write to the same workspace SQLite/Tantivy files and
    // corrupt the indexes silently.
    //
    // We perform this check BEFORE setting up logging so the
    // diagnostic goes to stderr where the operator can see it (no
    // log routing has been initialized yet).
    match crate::daemon::legacy_migration::check_or_refuse(&paths)? {
        crate::daemon::legacy_migration::MigrationDecision::LegacyDaemonAlive { pid, hint } => {
            eprintln!(
                "Refusing to start: legacy julie-server daemon is running (PID {}). \
                 Hint: {}. Stop it first via `julie-server stop` or `kill {}`.",
                pid, hint, pid
            );
            std::process::exit(2);
        }
        crate::daemon::legacy_migration::MigrationDecision::ProceedAndUnlink { files_to_clean } => {
            // Best-effort cleanup of legacy stale files. We
            // continue even on error: the new daemon's own
            // create_exclusive / try_acquire calls will fail
            // cleanly if any legacy file is still load-bearing.
            for path in files_to_clean {
                if let Err(err) = fs::remove_file(&path) {
                    eprintln!(
                        "Warning: failed to remove stale legacy file {}: {}",
                        path.display(),
                        err
                    );
                }
            }
        }
    }

    // Logging setup: file-only, no ANSI, daily rotation.
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("julie=info"))
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging filter: {}", e))?;

    let log_dir = paths.julie_home();
    fs::create_dir_all(&log_dir).unwrap_or_else(|e| {
        eprintln!("Failed to create log directory at {:?}: {}", log_dir, e);
    });

    let writer = crate::logging::LocalRollingWriter::new(&log_dir, "daemon.log");
    let (non_blocking_file, _file_guard) = non_blocking(writer);

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_writer(non_blocking_file)
                .with_timer(crate::logging::LocalTimer)
                .with_target(true)
                .with_ansi(false)
                .with_file(true)
                .with_line_number(true),
        )
        .init();

    info!("Starting Julie daemon v{}", env!("CARGO_PKG_VERSION"));
    crate::daemon::run_daemon(paths, port, no_dashboard).await
}

/// Stop the running daemon and print a confirmation.
///
/// Shared between `julie-daemon stop` and `julie-server stop`/`restart`.
pub fn stop_daemon(paths: &DaemonPaths) -> Result<()> {
    crate::daemon::lifecycle::stop_daemon(paths)?;
    println!("Daemon stopped");
    Ok(())
}

/// Print daemon status plus any recovery markers from previous unclean
/// shutdowns.
///
/// Shared between `julie-daemon status` and `julie-server status`.
pub fn status_daemon(paths: &DaemonPaths) {
    match crate::daemon::lifecycle::check_status(paths) {
        crate::daemon::lifecycle::DaemonStatus::Running { pid } => {
            println!("Julie daemon running (PID {})", pid);
        }
        crate::daemon::lifecycle::DaemonStatus::NotRunning => {
            println!("Julie daemon not running");
        }
    }
    // A1.7: surface any recovery markers from a previous unclean
    // shutdown so the operator knows in-flight requests were aborted
    // before the current daemon (if any) came up.
    let markers = crate::daemon::shutdown::read_recovery_markers(paths);
    if !markers.is_empty() {
        println!(
            "Recovery markers: {} unclean shutdown(s) from previous run(s):",
            markers.len()
        );
        for (idx, marker) in markers.iter().enumerate() {
            println!(
                "  [{}] {} active session(s) at timeout (drain={}s, ts_micros={})",
                idx,
                marker.active_sessions_at_timeout,
                marker.drain_timeout_secs,
                marker.shutdown_timestamp_micros,
            );
            if !marker.affected_workspaces.is_empty() {
                println!(
                    "        affected workspaces: {}",
                    marker.affected_workspaces.join(", ")
                );
            }
        }
        println!(
            "  (Clear with: rm {})",
            crate::daemon::shutdown::recovery_marker_path(paths).display()
        );
    }
}
