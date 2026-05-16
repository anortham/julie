//! `julie-daemon` — daemon lifecycle binary.
//!
//! Parses `start | stop | status` subcommands and dispatches to
//! `julie::daemon::cli::run`. All logic lives in `src/daemon/cli.rs` so it
//! can be tested without spawning a process.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    julie::daemon::cli::run().await
}
