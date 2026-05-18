//! `julie-adapter` — thin stdio↔HTTP forwarding binary.
//!
//! Parses only the `--workspace` flag (adapter mode does not interpret
//! subcommands), computes a `WorkspaceStartupHint`, and delegates to
//! `julie::adapter::run_adapter`.
//!
//! Judgment call: reuse `julie::cli::Cli` with `command: Option<Command>`
//! rather than a bespoke parser. `command` is ignored; the binary always runs
//! in adapter mode. This is shorter and keeps clap help consistent with
//! `julie-server` for the `--workspace` flag.

use clap::Parser;
use julie::cli::{Cli, resolve_workspace_startup_hint};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
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
