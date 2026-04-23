use std::fs;

use tracing::info;
use tracing_appender::{non_blocking, rolling};
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

            let file_appender = rolling::daily(&log_dir, "daemon.log");
            let (non_blocking_file, _file_guard) = non_blocking(file_appender);

            tracing_subscriber::registry()
                .with(filter)
                .with(
                    fmt::layer()
                        .with_writer(non_blocking_file)
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

        None => {
            // Adapter mode: auto-start daemon, forward stdio to IPC
            julie::adapter::run_adapter(startup_hint).await?;
        }
    }

    Ok(())
}

/// Route a tool command through the CLI execution core.
///
/// This wraps `run_cli_tool`, handles the output, and exits with the
/// appropriate status code. A4 will refine the output formatting.
async fn run_tool_command(
    command: &dyn julie::cli_tools::CliToolCommand,
    flags: &julie::cli_tools::GlobalToolFlags,
    cli_workspace: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let output = run_cli_tool(command, cli_workspace, flags.standalone).await?;

    // A4 will add proper formatting (text, json, markdown).
    // For now, dump the result JSON to stdout.
    let formatted = if flags.effective_format() == julie::cli_tools::OutputFormat::Json {
        serde_json::to_string_pretty(&output.result)?
    } else {
        // Extract text content from the CallToolResult structure
        extract_text_content(&output.result)
    };

    println!("{}", formatted);

    if output.is_error {
        std::process::exit(1);
    }

    Ok(())
}

/// Extract text content from a serialized CallToolResult for plain-text display.
/// A4 will replace this with proper formatting.
fn extract_text_content(result: &serde_json::Value) -> String {
    if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
        content
            .iter()
            .filter_map(|item| {
                // CallToolResult content items have a "text" field
                item.get("text").and_then(|t| t.as_str())
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        serde_json::to_string_pretty(result).unwrap_or_default()
    }
}
