//! `julie-server` — legacy entry point preserved as a compatibility shim.
//!
//! A1.8: the new world ships two binaries: `julie-adapter` (stdio↔HTTP forward)
//! and `julie-daemon` (lifecycle).  `julie-server` is kept as a single-binary
//! shim so existing plugin manifests, scripts, and operator muscle-memory
//! continue to work during the transition.
//!
//! Argv dispatch:
//!   - no args                 → adapter codepath (forward stdio to daemon)
//!   - `daemon`                → `julie-daemon start` codepath (start_daemon)
//!   - `stop` / `restart`      → `julie-daemon stop` codepath  (stop_daemon)
//!   - `status`                → `julie-daemon status` codepath (status_daemon)
//!   - `dashboard`             → open dashboard URL in browser (unchanged)
//!   - tool subcommands        → run_cli_tool (unchanged from today)
//!
//! All daemon lifecycle paths route through
//! `julie::daemon::cli::{start_daemon, stop_daemon, status_daemon}` so the
//! shim and `julie-daemon` share a single implementation. This is load-bearing:
//! `start_daemon` enforces the A1.5 legacy-migration gate, and skipping it
//! would re-open the silent-corruption window the gate exists to close.

use clap::Parser;
use julie::cli::{
    Cli, Command, cli_command_needs_workspace_startup_hint, dashboard_url_from_port_file_contents,
    resolve_workspace_startup_hint,
};
use julie::cli_tools::run_cli_tool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let needs_workspace_startup_hint = cli_command_needs_workspace_startup_hint(&cli.command);

    match cli.command {
        Some(Command::Daemon { port, no_dashboard }) => {
            // Route through the shared helper used by `julie-daemon start`.
            // Logging + legacy-migration gate + blocking run_daemon all live
            // in one place to keep the two entry points behaviorally identical.
            let paths = julie::paths::DaemonPaths::new();
            julie::daemon::cli::start_daemon(paths, port, no_dashboard).await?;
        }
        Some(Command::Dashboard) => {
            let paths = julie::paths::DaemonPaths::new();
            let port_file = paths.daemon_port();
            match std::fs::read_to_string(&port_file) {
                Ok(port) => match dashboard_url_from_port_file_contents(&port) {
                    Ok(url) => {
                        println!("Opening {}", url);
                        if let Err(e) = opener::open(&url) {
                            eprintln!("Failed to open browser: {}", e);
                            println!("Dashboard URL: {}", url);
                        }
                    }
                    Err(e) => {
                        eprintln!("Dashboard not available: {e}");
                        std::process::exit(1);
                    }
                },
                Err(_) => {
                    eprintln!("Dashboard not available. Is the daemon running?");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Stop) => {
            let paths = julie::paths::DaemonPaths::new();
            julie::daemon::cli::stop_daemon(&paths)?;
        }
        Some(Command::Status) => {
            let paths = julie::paths::DaemonPaths::new();
            julie::daemon::cli::status_daemon(&paths);
        }
        Some(Command::Restart) => {
            let paths = julie::paths::DaemonPaths::new();
            julie::daemon::cli::stop_daemon(&paths)?;
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
        Some(Command::CallPath(args)) => {
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
            debug_assert!(needs_workspace_startup_hint);
            let startup_hint = resolve_workspace_startup_hint(cli.workspace);
            // Adapter mode: auto-start daemon (via launcher → `julie-daemon
            // start` per A1.8), forward stdio to HTTP MCP.
            //
            // Install adapter-side file tracing before run_adapter so all
            // launcher info!/warn!/error! land in ~/.julie/adapter.log.
            // Without this, the adapter is silent — operators have no way
            // to see why a cold-start spawn is slow, retried, or failing.
            // Logs go to a separate file from the daemon so the daemon's
            // own subscriber (installed in start_daemon) is not affected.
            let paths = julie::paths::DaemonPaths::new();
            if let Err(e) = julie::logging::install_file_tracing(
                &paths.julie_home(),
                "adapter.log",
                "julie=info",
            ) {
                // Never fail the adapter over logging — stderr is fine for
                // this last-resort signal; MCP protocol owns stdout only.
                eprintln!("Julie adapter: failed to install file tracing: {}", e);
            }
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
