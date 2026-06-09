//! `julie-embedding-host` — resident embedding host process.
//!
//! Acquires the singleton lock, binds the IPC front door, and serves
//! `health` / `embed_query` / `embed_batch` requests until SIGTERM / SIGINT.
//! All logic lives in `julie_pipeline::embeddings::host_server`.

use anyhow::Context as _;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Resolve $JULIE_HOME (or the platform default) into structured paths.
    let paths = julie::paths::RegistryPaths::try_new()
        .map_err(|e| anyhow::anyhow!("JULIE_HOME misconfiguration: {e}"))?;

    // File tracing → $JULIE_HOME/embedding-host.<date>.log
    if let Err(e) =
        julie::logging::install_file_tracing(&paths.julie_home(), "embedding-host", "julie=info")
    {
        eprintln!("julie-embedding-host: failed to install file tracing: {e}");
    }

    info!("julie-embedding-host starting");

    let addr = julie_pipeline::embeddings::host_transport::HostAddress::from_paths(&paths);
    let lock_path = paths.embedding_host_lock();

    let cancel = CancellationToken::new();

    // Spawn a task that cancels the token when a shutdown signal arrives.
    let cancel_signal = cancel.clone();
    tokio::spawn(async move {
        match await_shutdown_signal().await {
            Ok(()) => info!("shutdown signal received"),
            Err(e) => error!("signal handler error: {e}"),
        }
        cancel_signal.cancel();
    });

    julie_pipeline::embeddings::host_server::run_embedding_host_default(&addr, &lock_path, cancel)
        .await
}

// ---------------------------------------------------------------------------
// Signal handling (mirrors daemon's shutdown_signal, inlined because
// that function is pub(crate) and bins are separate compilation units)
// ---------------------------------------------------------------------------

async fn await_shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm =
            signal(SignalKind::terminate()).context("failed to register SIGTERM handler")?;
        let mut sigint =
            signal(SignalKind::interrupt()).context("failed to register SIGINT handler")?;
        tokio::select! {
            _ = sigterm.recv() => {}
            _ = sigint.recv() => {}
        }
    }
    #[cfg(windows)]
    {
        tokio::signal::ctrl_c()
            .await
            .context("failed to listen for Ctrl-C")?;
    }
    Ok(())
}
