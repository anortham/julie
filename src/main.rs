use std::fs;

use tracing::{debug, error, info, warn};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use clap::Parser;
use julie::cli::{Cli, resolve_workspace_root};
use julie::handler::JulieServerHandler;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let workspace_root = resolve_workspace_root(cli.workspace);

    // Initialize logging — file only, stdout reserved for MCP JSON-RPC
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("julie=info"))
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging filter: {}", e))?;

    let logs_dir = workspace_root.join(".julie").join("logs");
    fs::create_dir_all(&logs_dir).unwrap_or_else(|e| {
        eprintln!("Failed to create logs directory at {:?}: {}", logs_dir, e);
    });

    let file_appender = rolling::daily(&logs_dir, "julie.log");
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

    info!("Starting Julie v{} (stdio mode)", env!("CARGO_PKG_VERSION"));
    info!("Workspace root: {:?}", workspace_root);

    // Create handler and start stdio MCP transport
    let handler = JulieServerHandler::new(workspace_root)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create handler: {}", e))?;
    let cleanup_handler = handler.clone();

    let service = match handler.serve(stdio()).await {
        Ok(s) => s,
        Err(e) => {
            error!("Server failed to start: {}", e);
            return Err(anyhow::anyhow!("Server failed to start: {}", e));
        }
    };

    if let Err(e) = service.waiting().await {
        error!("Server error: {}", e);
        return Err(anyhow::anyhow!("Server error: {}", e));
    }

    info!("Julie server stopped");

    match julie::startup::checkpoint_active_workspace_wal(&cleanup_handler).await {
        Ok(Some((busy, log, checkpointed))) => {
            info!(
                "WAL checkpoint complete: busy={}, log={}, checkpointed={}",
                busy, log, checkpointed
            );
        }
        Ok(None) => {
            debug!("No database available for shutdown checkpoint");
        }
        Err(e) => {
            warn!("WAL checkpoint failed: {}", e);
        }
    }

    Ok(())
}
