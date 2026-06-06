//! `julie-server` — the MCP entry point.
//!
//! Post-cutover (Phase 3c.3) the no-args invocation serves the MCP handler
//! IN-PROCESS over stdio (leader-locked; no daemon fork). The `julie-adapter`
//! stdio↔HTTP bridge was removed in Phase 3d.1; the `julie daemon` entry and
//! `julie-daemon` binary were removed in Phase 3d.2a.
//!
//! Argv dispatch:
//!   - no args                 → in-process MCP server (run_in_process_server)
//!   - `dashboard`             → serve standalone read-only dashboard
//!   - tool subcommands        → run_cli_tool (standalone, in-process)

use clap::Parser;
use julie::cli::{
    Cli, Command, cli_command_needs_workspace_startup_hint, resolve_workspace_startup_hint,
};
use julie::cli_tools::run_cli_tool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let needs_workspace_startup_hint = cli_command_needs_workspace_startup_hint(&cli.command);

    match cli.command {
        Some(Command::Dashboard) => {
            julie::dashboard::standalone::serve_dashboard_forever().await?;
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
        Some(Command::Extract(raw_args)) => {
            run_extract_command(raw_args, &cli.tool_flags).await?;
        }

        None => {
            debug_assert!(needs_workspace_startup_hint);
            let startup_hint = resolve_workspace_startup_hint(cli.workspace);
            // THE CUTOVER (Phase 3c.3, T10): serve `JulieServerHandler` directly
            // over rmcp stdio — IN-PROCESS. No daemon fork, no stdio↔HTTP bridge,
            // no `discovery.json`. Each process wins or loses a per-workspace OS
            // leader lock; the winner is the sole file watcher + Tantivy writer,
            // losers are pure SQLite-WAL + Tantivy-mmap readers
            // (`run_in_process_server`). The daemon/adapter code paths remain
            // compiled and reachable via the other subcommands — bypassed, not
            // deleted (T12 tripwire pins this boundary).
            //
            // Install per-project file tracing at `<project>/.julie/logs/julie.log`
            // BEFORE serving so startup/indexing diagnostics are captured. The
            // in-process server is the project's OWN server now, so its log lives
            // with the project (not the shared `~/.julie/adapter.log` the old
            // adapter used). Logging must NEVER fail startup — MCP owns stdout, so
            // a logging error is reported to stderr and otherwise ignored.
            let log_dir = startup_hint.path.join(".julie").join("logs");
            let _ = std::fs::create_dir_all(&log_dir);
            if let Err(e) =
                julie::logging::install_file_tracing(&log_dir, "julie.log", "julie=info")
            {
                eprintln!("Julie in-process server: failed to install file tracing: {e}");
            }
            julie::server_in_process::run_in_process_server(startup_hint).await?;
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

/// Run external extraction against a caller-owned SQLite database.
async fn run_extract_command(
    raw_args: julie::external_extract::ExternalExtractRawArgs,
    flags: &julie::cli_tools::GlobalToolFlags,
) -> anyhow::Result<()> {
    let args = raw_args.validate().unwrap_or_else(|error| error.exit());
    let report = match julie::external_extract::run_external_extract(&args).await {
        Ok(report) => report,
        Err(error) => {
            let report = julie::external_extract::failed_external_extract_report(&args, &error);
            let formatted = julie::external_extract::format_external_extract_report(
                &report,
                flags.effective_format(),
            )?;
            println!("{}", formatted);
            std::process::exit(1);
        }
    };

    let formatted =
        julie::external_extract::format_external_extract_report(&report, flags.effective_format())?;
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
