//! Adapter: the thin process that MCP clients spawn.
//!
//! The adapter auto-starts the daemon if not running, connects to the IPC
//! socket, sends a workspace header, then bidirectionally forwards bytes
//! between stdin/stdout and the daemon. From the MCP client's perspective,
//! it looks exactly like a stdio MCP server.

pub mod launcher;

use std::path::PathBuf;

use anyhow::{Context, Result};
use tokio::io::{AsyncWriteExt, copy};
use tracing::{error, info};

use crate::daemon::ipc::{IpcClientStream, IpcConnector};
use crate::paths::DaemonPaths;

use self::launcher::DaemonLauncher;

/// Run the adapter: auto-start daemon, connect, forward bytes.
///
/// This is the default entry point when no subcommand is given. It:
/// 1. Ensures the daemon is running (spawning it if necessary)
/// 2. Connects to the daemon's IPC socket
/// 3. Sends the workspace header (`WORKSPACE:/path\n`)
/// 4. Bidirectionally forwards stdin/stdout to/from the IPC stream
///
/// On connection loss, logs the error and exits cleanly. The MCP client
/// is responsible for restarting the adapter process — transparent reconnect
/// from the adapter side breaks MCP session state.
pub async fn run_adapter(workspace_root: PathBuf) -> Result<()> {
    let paths = DaemonPaths::new();
    let launcher = DaemonLauncher::new(paths.clone());

    // ensure_daemon_running blocks (file locks + thread::sleep poll).
    // block_in_place yields the tokio thread pool slot while blocking.
    tokio::task::block_in_place(|| launcher.ensure_daemon_running())
        .context("Failed to ensure daemon is running")?;

    let stream = connect_and_handshake(&paths, &workspace_root).await?;
    info!("Adapter connected to daemon, forwarding bytes");

    match forward_bytes(stream).await {
        Ok(()) => {
            info!("Adapter session ended normally");
            Ok(())
        }
        Err(e) => {
            error!("Adapter connection lost: {}", e);
            // Exit cleanly; the MCP client will restart the adapter
            Ok(())
        }
    }
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
    // Normalize to forward slashes so the workspace ID is consistent on Windows.
    let header = format!(
        "WORKSPACE:{}\n",
        workspace_root.to_string_lossy().replace('\\', "/")
    );
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
