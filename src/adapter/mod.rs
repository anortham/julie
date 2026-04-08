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
/// On connection loss during forwarding, logs the error and exits cleanly.
/// The MCP client is responsible for restarting the adapter process in that
/// case; transparent reconnect from the adapter side would break MCP session
/// state.
///
/// However, initial connection failures ARE retried: the daemon may have just
/// shut down for a stale-binary restart, and the adapter needs to re-launch it.
/// Without this retry, the MCP client sees "failed" on every rebuild cycle.
pub async fn run_adapter(workspace_root: PathBuf) -> Result<()> {
    let paths = DaemonPaths::new();
    let launcher = DaemonLauncher::new(paths.clone());

    // Retry loop for initial connection. The daemon may reject our connection
    // if it detects a stale binary (rebuilt since daemon started). In that case
    // it shuts down before accepting, so connect_and_handshake fails. We wait
    // briefly for the old daemon to exit, then re-launch and reconnect.
    const MAX_RETRIES: u32 = 2;
    let mut stream = None;
    for attempt in 0..=MAX_RETRIES {
        tokio::task::block_in_place(|| launcher.ensure_daemon_ready())
            .context("Failed to ensure daemon is ready")?;

        match connect_and_handshake(&paths, &workspace_root).await {
            Ok(s) => {
                stream = Some(s);
                break;
            }
            Err(e) => {
                if attempt < MAX_RETRIES {
                    info!(
                        "Connection attempt {} failed ({}), retrying after daemon restart...",
                        attempt + 1, e
                    );
                    // Give the old daemon time to fully shut down and release
                    // the socket before we try to spawn a new one.
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                } else {
                    return Err(e).context("Failed to connect to daemon after retries");
                }
            }
        }
    }
    let stream = stream.expect("loop guarantees stream is set on success");

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

    // Send IPC headers: workspace path, then adapter version.
    // Path is sent as-is (native format); generate_workspace_id() normalizes
    // internally so the workspace ID is consistent regardless of separators.
    let header = format!(
        "WORKSPACE:{}\nVERSION:{}\n",
        workspace_root.to_string_lossy(),
        env!("CARGO_PKG_VERSION"),
    );
    stream
        .write_all(header.as_bytes())
        .await
        .context("Failed to send IPC headers")?;

    Ok(stream)
}

/// Bidirectional byte forwarding between stdin/stdout and the IPC stream.
///
/// Both directions run concurrently. When stdin closes (MCP client sent its
/// last message or exited), we shut down the IPC write side to signal the
/// daemon, then keep draining daemon-to-stdout until the daemon is done.
/// When stdout closes (client gone), we stop immediately.
async fn forward_bytes(stream: IpcClientStream) -> Result<()> {
    let (mut ipc_read, mut ipc_write) = tokio::io::split(stream);
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    // Run both directions concurrently with tokio::select!, but when
    // stdin->daemon finishes, don't return immediately. Instead, fall
    // through to drain the daemon->stdout direction.
    let stdout_result;
    tokio::select! {
        result = copy(&mut stdin, &mut ipc_write) => {
            match result {
                Ok(bytes) => info!("stdin->daemon forwarding ended ({} bytes)", bytes),
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof
                        || e.kind() == std::io::ErrorKind::BrokenPipe
                    {
                        info!("stdin closed (client exited)");
                    } else {
                        info!("stdin->daemon forwarding error: {}", e);
                    }
                }
            }
            // Signal daemon that no more input is coming
            let _ = ipc_write.shutdown().await;
            // Now drain the daemon's remaining output to stdout
            stdout_result = copy(&mut ipc_read, &mut stdout).await;
        }
        result = copy(&mut ipc_read, &mut stdout) => {
            // Daemon closed its side or stdout broke. Nothing more to do.
            stdout_result = result;
        }
    }

    match stdout_result {
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
