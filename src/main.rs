use std::fs;

use tracing::info;
use tracing_appender::non_blocking;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use clap::Parser;
use julie::cli::{Cli, Command, resolve_workspace_startup_hint};
use julie::cli_tools::run_cli_tool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let startup_hint = resolve_workspace_startup_hint(cli.workspace.clone());

    match cli.command {
        Some(Command::Daemon { port, no_dashboard }) => {
            let paths = julie::paths::DaemonPaths::new();

            // Set up daemon logging (to ~/.julie/daemon.log)
            let filter = EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("julie=info"))
                .map_err(|e| anyhow::anyhow!("Failed to initialize logging filter: {}", e))?;

            let log_dir = paths.julie_home();
            fs::create_dir_all(&log_dir).unwrap_or_else(|e| {
                eprintln!("Failed to create log directory at {:?}: {}", log_dir, e);
            });

            let writer = julie::logging::LocalRollingWriter::new(&log_dir, "daemon.log");
            let (non_blocking_file, _file_guard) = non_blocking(writer);

            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .with_writer(non_blocking_file)
                        .with_timer(julie::logging::LocalTimer)
                        .with_target(true)
                        .with_ansi(false)
                        .with_file(true)
                        .with_line_number(true),
                )
                .init();

            info!("Starting Julie daemon v{}", env!("CARGO_PKG_VERSION"));
            julie::daemon::run_daemon(paths, port, no_dashboard).await?;
        }
        Some(Command::Dashboard) => {
            let paths = julie::paths::DaemonPaths::new();
            let port_file = paths.daemon_port();
            match std::fs::read_to_string(&port_file) {
                Ok(port) => {
                    let url = format!("http://localhost:{}", port.trim());
                    println!("Opening {}", url);
                    if let Err(e) = opener::open(&url) {
                        eprintln!("Failed to open browser: {}", e);
                        println!("Dashboard URL: {}", url);
                    }
                }
                Err(_) => {
                    eprintln!("Dashboard not available. Is the daemon running?");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Stop) => {
            let paths = julie::paths::DaemonPaths::new();
            julie::daemon::lifecycle::stop_daemon(&paths)?;
            println!("Daemon stopped");
        }
        Some(Command::Status) => {
            let paths = julie::paths::DaemonPaths::new();
            match julie::daemon::lifecycle::check_status(&paths) {
                julie::daemon::lifecycle::DaemonStatus::Running { pid } => {
                    println!("Julie daemon running (PID {})", pid);
                }
                julie::daemon::lifecycle::DaemonStatus::NotRunning => {
                    println!("Julie daemon not running");
                }
            }
        }
        Some(Command::Restart) => {
            let paths = julie::paths::DaemonPaths::new();
            julie::daemon::lifecycle::stop_daemon(&paths)?;
            println!("Daemon stopped. Will auto-restart on next tool call.");
        }

        // Tool commands: routed through the CLI execution core
        Some(Command::Search(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::Refs(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::Symbols(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::Context(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::BlastRadius(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::Workspace(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::Tool(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::Signals(args)) => {
            run_signals_command(&args, &cli.tool_flags, cli.workspace).await?;
        }

        None => {
            // Adapter mode: auto-start daemon, forward stdio to IPC
            julie::adapter::run_adapter(startup_hint).await?;
        }
    }

    Ok(())
}

/// Run the early warning signals report (standalone-only, not an MCP tool).
async fn run_signals_command(
    args: &julie::cli_tools::subcommands::SignalsArgs,
    flags: &julie::cli_tools::GlobalToolFlags,
    cli_workspace: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let output = julie::cli_tools::run_signals_report(args, cli_workspace).await?;
    let formatted =
        julie::cli_tools::output::format_signals_report(&output, flags.effective_format());
    println!("{}", formatted);
    Ok(())
}

/// Route a tool command through the CLI execution core.
///
/// Formats output according to `--format` / `--json` flags, prints to stdout,
/// and exits with code 1 if the tool reported an error.
async fn run_tool_command(
    command: &dyn julie::cli_tools::CliToolCommand,
    flags: &julie::cli_tools::GlobalToolFlags,
    cli_workspace: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let output = run_cli_tool(command, cli_workspace, flags.standalone).await?;

    let formatted = julie::cli_tools::output::format_output(
        &output,
        flags.effective_format(),
        command.tool_name(),
    );

    println!("{}", formatted);

    if output.is_error {
        std::process::exit(1);
    }

    Ok(())
}
