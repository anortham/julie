//! Adapter: the thin process that MCP clients spawn.
//!
//! The adapter auto-starts the daemon if not running, connects to the IPC
//! socket, sends a workspace header, then bidirectionally forwards bytes
//! between stdin/stdout and the daemon. From the MCP client's perspective,
//! it looks exactly like a stdio MCP server.

pub mod launcher;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncWriteExt, copy};
use tracing::{error, info, warn};

use crate::daemon::ipc::{IpcClientStream, IpcConnector};
use crate::paths::DaemonPaths;

use self::launcher::DaemonLauncher;

/// Maximum number of reconnect attempts before re-running the full startup.
const MAX_RECONNECT_ATTEMPTS: u32 = 3;

/// Base delay for exponential backoff during reconnect (doubles each attempt).
const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);

/// Run the adapter: auto-start daemon, connect, forward bytes.
///
/// This is the default entry point when no subcommand is given. It:
/// 1. Ensures the daemon is running (spawning it if necessary)
/// 2. Connects to the daemon's IPC socket
/// 3. Sends the workspace header (`WORKSPACE:/path\n`)
/// 4. Bidirectionally forwards stdin/stdout to/from the IPC stream
/// 5. On disconnect, attempts reconnect with exponential backoff
pub async fn run_adapter(workspace_root: PathBuf) -> Result<()> {
    let paths = DaemonPaths::new();
    let launcher = DaemonLauncher::new(paths.clone());

    // Ensure daemon is running (spawns if needed, waits for socket)
    launcher
        .ensure_daemon_running()
        .context("Failed to ensure daemon is running")?;

    // Connect and forward. On disconnect, try to reconnect.
    run_with_reconnect(&launcher, &paths, &workspace_root).await
}

/// Connect to daemon and forward bytes. Reconnects on disconnect.
async fn run_with_reconnect(
    launcher: &DaemonLauncher,
    paths: &DaemonPaths,
    workspace_root: &PathBuf,
) -> Result<()> {
    // Initial connection
    let stream = connect_and_handshake(paths, workspace_root).await?;
    info!("Adapter connected to daemon, forwarding bytes");

    let result = forward_bytes(stream).await;

    match result {
        Ok(()) => {
            // Clean shutdown (stdin closed, or daemon closed connection gracefully)
            info!("Adapter session ended normally");
            return Ok(());
        }
        Err(e) => {
            warn!("Adapter connection lost: {}", e);
        }
    }

    // Attempt reconnect with exponential backoff
    for attempt in 1..=MAX_RECONNECT_ATTEMPTS {
        let delay = RECONNECT_BASE_DELAY * 2u32.pow(attempt - 1);
        warn!(
            "Reconnect attempt {}/{} in {}ms...",
            attempt,
            MAX_RECONNECT_ATTEMPTS,
            delay.as_millis()
        );
        tokio::time::sleep(delay).await;

        match connect_and_handshake(paths, workspace_root).await {
            Ok(stream) => {
                info!("Reconnected on attempt {}", attempt);
                match forward_bytes(stream).await {
                    Ok(()) => {
                        info!("Adapter session ended normally after reconnect");
                        return Ok(());
                    }
                    Err(e) => {
                        warn!("Connection lost again after reconnect: {}", e);
                        continue;
                    }
                }
            }
            Err(e) => {
                warn!("Reconnect attempt {} failed: {}", attempt, e);
            }
        }
    }

    // All retries exhausted; re-run the full ensure_daemon_running sequence
    warn!("All reconnect attempts failed, re-running full daemon startup");
    launcher
        .ensure_daemon_running()
        .context("Failed to restart daemon after reconnect failures")?;

    let stream = connect_and_handshake(paths, workspace_root)
        .await
        .context("Failed to connect after full daemon restart")?;

    info!("Connected after full daemon restart");
    forward_bytes(stream).await.map_err(|e| {
        error!("Final connection failed: {}", e);
        anyhow::anyhow!(
            "Adapter connection failed after all recovery attempts: {}",
            e
        )
    })
}

/// Connect to the daemon IPC endpoint and send the workspace header.
async fn connect_and_handshake(
    paths: &DaemonPaths,
    workspace_root: &PathBuf,
) -> Result<IpcClientStream> {
    let ipc_addr = paths.daemon_ipc_addr();
    let mut stream = IpcConnector::connect(&ipc_addr)
        .await
        .context("Failed to connect to daemon IPC endpoint")?;

    // Send workspace header: WORKSPACE:/path/to/project\n
    let header = format!("WORKSPACE:{}\n", workspace_root.display());
    stream
        .write_all(header.as_bytes())
        .await
        .context("Failed to send workspace header")?;

    Ok(stream)
}

/// Bidirectional byte forwarding between stdin/stdout and the IPC stream.
///
/// Uses `tokio::io::copy` in both directions simultaneously via `tokio::select!`.
/// When either direction finishes (EOF or error), the other is cancelled and
/// we return.
async fn forward_bytes(stream: IpcClientStream) -> Result<()> {
    let (mut ipc_read, mut ipc_write) = tokio::io::split(stream);
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    tokio::select! {
        result = copy(&mut stdin, &mut ipc_write) => {
            match result {
                Ok(bytes) => {
                    info!("stdin->daemon forwarding ended ({} bytes)", bytes);
                    Ok(())
                }
                Err(e) => {
                    // stdin closing is normal (MCP client exited)
                    if e.kind() == std::io::ErrorKind::UnexpectedEof
                        || e.kind() == std::io::ErrorKind::BrokenPipe
                    {
                        info!("stdin closed (client exited)");
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("stdin->daemon forwarding error: {}", e))
                    }
                }
            }
        }
        result = copy(&mut ipc_read, &mut stdout) => {
            match result {
                Ok(bytes) => {
                    info!("daemon->stdout forwarding ended ({} bytes)", bytes);
                    Ok(())
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::BrokenPipe {
                        info!("stdout closed (client exited)");
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("daemon->stdout forwarding error: {}", e))
                    }
                }
            }
        }
    }
}
