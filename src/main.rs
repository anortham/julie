//! `julie-server` — the MCP entry point.
//!
//! Serves the in-process MCP handler over stdio or executes CLI tool commands.

use clap::Parser;
use std::sync::Arc;

use julie::cli::{Cli, Command};
use julie::cli_tools::output::{
    format_failure_envelope, format_success_envelope, write_stdout_safe,
};
use julie::cli_tools::run_cli_tool;
use julie::cli_tools::subcommands::ToolsSubcommand;
use julie::request_engine::{
    BindingResolver, RequestEngine, RequestFailure, RequestReadiness, RuntimeFactory, ToolReply,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Err(e) = run_main(cli).await {
        if let Some(ce) = e.downcast_ref::<julie::service::client::ConnectError>() {
            eprintln!("{ce}");
            std::process::exit(julie::service::client::exit_code(ce));
        }
        return Err(e);
    }
    Ok(())
}

async fn run_main(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Some(Command::Dashboard) => {
            julie::dashboard::standalone::open_dashboard().await?;
        }
        Some(Command::Tools(tools_args)) => {
            run_tools_command(&tools_args, &cli.tool_flags, cli.workspace).await?;
        }
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
        Some(Command::Patterns(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::DeepDive(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::Edit(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::Rename(args)) => {
            run_tool_command(&args, &cli.tool_flags, cli.workspace).await?;
        }
        Some(Command::Rewrite(args)) => {
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
        Some(Command::Service(args)) => {
            run_service_command(&args).await?;
        }

        Some(Command::McpStdio) | None => {
            julie::service::shim::run_stdio_shim().await?;
        }
    }

    Ok(())
}

async fn run_service_command(args: &julie::cli::ServiceArgs) -> anyhow::Result<()> {
    match &args.action {
        None => julie::service::run_service(julie::service::ServiceConfig::from_env()?).await?,
        Some(julie::cli::ServiceAction::Status) => {
            let paths = julie_core::paths::RegistryPaths::try_new()?;
            let client = julie::service::client::connect_or_start(
                &paths,
                julie::service::client::spawn_detached_service,
            )
            .await?;
            let status = client.status().await?;
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
        Some(julie::cli::ServiceAction::Stop) => {
            let paths = julie_core::paths::RegistryPaths::try_new()?;
            if let Ok(Some(record)) = julie::service::discovery::read_record(&paths) {
                let client = julie::service::client::ServiceClient::from_record(&record);
                let _ = client.post_shutdown().await;
            }
            println!("stopped");
        }
        Some(julie::cli::ServiceAction::Restart) => {
            let paths = julie_core::paths::RegistryPaths::try_new()?;
            if let Ok(Some(record)) = julie::service::discovery::read_record(&paths) {
                let client = julie::service::client::ServiceClient::from_record(&record);
                let _ = client.post_shutdown().await;
                for _ in 0..50 {
                    if julie::service::discovery::read_record(&paths)?.is_none() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }
            let client = julie::service::client::connect_or_start(
                &paths,
                julie::service::client::spawn_detached_service,
            )
            .await?;
            let status = client.status().await?;
            println!("{}", serde_json::to_string_pretty(&status)?);
        }
    }
    Ok(())
}

/// Run tools discovery (list/schema) and replay subcommands.
async fn run_tools_command(
    tools_args: &julie::cli_tools::subcommands::ToolsArgs,
    flags: &julie::cli_tools::GlobalToolFlags,
    cli_workspace: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    match &tools_args.command {
        ToolsSubcommand::List => {
            match julie::cli_tools::catalog::run_tools_list(flags.effective_format()) {
                Ok((text, code)) => {
                    write_stdout_safe(&text);
                    std::process::exit(code);
                }
                Err(failure) => {
                    if flags.json {
                        write_stdout_safe(&format_failure_envelope(None, &failure));
                    } else {
                        eprintln!("Error: {}", failure.message);
                    }
                    std::process::exit(failure.exit_code());
                }
            }
        }
        ToolsSubcommand::Schema(schema_args) => {
            match julie::cli_tools::catalog::run_tools_schema(
                &schema_args.name,
                flags.effective_format(),
            ) {
                Ok((text, code)) => {
                    write_stdout_safe(&text);
                    std::process::exit(code);
                }
                Err(failure) => {
                    if flags.json {
                        write_stdout_safe(&format_failure_envelope(None, &failure));
                    } else {
                        eprintln!("Error: {}", failure.message);
                    }
                    std::process::exit(failure.exit_code());
                }
            }
        }
        ToolsSubcommand::Replay(replay_args) => {
            let registry_paths = julie::paths::RegistryPaths::default();
            let binding_resolver =
                BindingResolver::new(cli_workspace, true, registry_paths.clone());
            let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths));
            let engine = Arc::new(RequestEngine::new(binding_resolver, runtime_factory));

            let code = julie::cli_tools::replay::run_replay(engine, replay_args, false).await?;
            std::process::exit(code);
        }
    }
}

/// Run the early warning signals report (standalone-only, not an MCP tool).
async fn run_signals_command(
    args: &julie::cli_tools::subcommands::SignalsArgs,
    flags: &julie::cli_tools::GlobalToolFlags,
    cli_workspace: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let output = julie::cli_tools::run_signals_report(args, cli_workspace).await?;
    let formatted =
        julie::cli_tools::signals_output::format_signals_report(&output, flags.effective_format());
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
async fn run_tool_command(
    command: &dyn julie::cli_tools::CliToolCommand,
    flags: &julie::cli_tools::GlobalToolFlags,
    cli_workspace: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let (tool_name, arguments) = match command.to_request() {
        Ok(pair) => pair,
        Err(failure) => {
            if flags.json {
                write_stdout_safe(&format_failure_envelope(None, &failure));
            } else {
                eprintln!("Error: {}", failure.message);
            }
            std::process::exit(failure.exit_code());
        }
    };

    // Dashboard foreground check
    if tool_name == "manage_workspace"
        && arguments.get("operation").and_then(|v| v.as_str()) == Some("dashboard")
    {
        let is_foreground = arguments
            .get("foreground")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if is_foreground {
            return julie::dashboard::standalone::open_dashboard().await;
        } else {
            let failure = RequestFailure::foreground_required(
                "Interactive dashboard requires foreground mode. Run with --foreground flag (e.g., `julie-server workspace dashboard --foreground` or `julie-server dashboard`).",
            );
            if flags.json {
                write_stdout_safe(&format_failure_envelope(None, &failure));
            } else {
                eprintln!("Error: {}", failure.message);
            }
            std::process::exit(failure.exit_code());
        }
    }

    match run_cli_tool(command, cli_workspace, flags.standalone).await {
        Ok(output) => {
            if flags.json {
                let reply = ToolReply::from_result(
                    command.tool_name(),
                    None,
                    output.result,
                    RequestReadiness::ready(julie::request_engine::SemanticMode::Auto),
                );
                let exit_code = if output.is_error { 3 } else { 0 };
                write_stdout_safe(&format_success_envelope(None, &reply));
                std::process::exit(exit_code);
            } else {
                let formatted = julie::cli_tools::output::format_output(
                    &output,
                    flags.effective_format(),
                    command.tool_name(),
                );
                println!("{}", formatted);
                if output.is_error {
                    std::process::exit(3);
                }
            }
        }
        Err(failure) => {
            let exit_code = failure.exit_code();
            if flags.json {
                write_stdout_safe(&format_failure_envelope(None, &failure));
            } else {
                eprintln!("Error: {}", failure.message);
            }
            std::process::exit(exit_code);
        }
    }

    Ok(())
}
