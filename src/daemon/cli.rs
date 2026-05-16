//! CLI entry point for the `julie-daemon` binary.
//!
//! Parses `start | stop | status` from argv and dispatches to the appropriate
//! daemon lifecycle function.  All three subcommands delegate to existing code
//! paths so behavior is identical to `julie-server daemon`, `julie-server stop`,
//! and `julie-server status` today.

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
        /// HTTP port for the daemon (default: 7890, fallback to auto if taken)
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
        DaemonCommand::Start { port, no_dashboard } => {
            // Logging setup: identical to the `Command::Daemon` branch in
            // `src/main.rs` — file-only, no ANSI, daily rotation.
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
            crate::daemon::run_daemon(paths, port, no_dashboard).await?;
        }

        DaemonCommand::Stop => {
            crate::daemon::lifecycle::stop_daemon(&paths)?;
            println!("Daemon stopped");
        }

        DaemonCommand::Status => {
            match crate::daemon::lifecycle::check_status(&paths) {
                crate::daemon::lifecycle::DaemonStatus::Running { pid } => {
                    println!("Julie daemon running (PID {})", pid);
                }
                crate::daemon::lifecycle::DaemonStatus::NotRunning => {
                    println!("Julie daemon not running");
                }
            }
        }
    }

    Ok(())
}
