//! `julie-adapter` — thin stdio↔HTTP forwarding binary.
//!
//! Parses only the `--workspace` flag (adapter mode does not interpret
//! subcommands), computes a `WorkspaceStartupHint`, and delegates to
//! `julie::adapter::run_adapter`.

use std::path::PathBuf;

use clap::Parser;
use julie::cli::resolve_workspace_startup_hint;

#[derive(Debug, Parser)]
#[command(
    name = "julie-adapter",
    version,
    about = "Forward stdio MCP traffic to the Julie daemon"
)]
struct AdapterCli {
    /// Workspace root to open or register before serving requests.
    #[arg(long, value_name = "PATH")]
    workspace: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = AdapterCli::parse();
    let startup_hint = resolve_workspace_startup_hint(cli.workspace);

    // Install adapter-side file tracing so launcher info!/warn!/error! land in
    // ~/.julie/adapter.log. Without this the adapter is silent — operators have
    // no way to see why a cold-start spawn is slow, retried, or failing. Kept
    // in sync with `src/main.rs` no-args branch (the shim binary).
    let paths = julie::paths::DaemonPaths::new();
    if let Err(e) =
        julie::logging::install_file_tracing(&paths.julie_home(), "adapter.log", "julie=info")
    {
        eprintln!("Julie adapter: failed to install file tracing: {}", e);
    }

    julie::adapter::run_adapter(startup_hint).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn adapter_cli_accepts_no_args_and_workspace_only() {
        let no_args = AdapterCli::try_parse_from(["julie-adapter"]).expect("no args parse");
        assert!(no_args.workspace.is_none());

        let with_workspace =
            AdapterCli::try_parse_from(["julie-adapter", "--workspace", "/tmp/project"])
                .expect("workspace flag parses");
        assert_eq!(
            with_workspace.workspace,
            Some(PathBuf::from("/tmp/project"))
        );
    }

    #[test]
    fn adapter_cli_rejects_server_commands_and_tool_flags() {
        for args in [
            ["julie-adapter", "status"].as_slice(),
            ["julie-adapter", "search", "needle"].as_slice(),
            ["julie-adapter", "--standalone"].as_slice(),
            ["julie-adapter", "--json"].as_slice(),
        ] {
            assert!(
                AdapterCli::try_parse_from(args).is_err(),
                "adapter CLI must reject {:?}",
                args
            );
        }
    }

    #[test]
    fn adapter_cli_help_exposes_only_adapter_options() {
        let help = AdapterCli::command().render_long_help().to_string();

        assert!(help.contains("--workspace"), "help: {help}");
        assert!(help.contains("--help"), "help: {help}");
        assert!(help.contains("--version"), "help: {help}");
        assert!(!help.contains("status"), "help: {help}");
        assert!(!help.contains("search"), "help: {help}");
        assert!(!help.contains("--standalone"), "help: {help}");
        assert!(!help.contains("--json"), "help: {help}");
    }
}
