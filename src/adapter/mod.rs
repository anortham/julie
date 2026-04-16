//! Adapter: the thin process that MCP clients spawn.
//!
//! The adapter auto-starts the daemon if not running, connects to the IPC
//! socket, sends a workspace header, then bidirectionally forwards bytes
//! between stdin/stdout and the daemon. From the MCP client's perspective,
//! it looks exactly like a stdio MCP server.

pub mod launcher;

use anyhow::{Context, Result};
use std::future::Future;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, copy};
use tracing::{error, info};

use crate::daemon::ipc::{IpcClientStream, IpcConnector};
use crate::daemon::lifecycle::{RestartHandoffAction, RestartReason, restart_handoff_action};
use crate::paths::DaemonPaths;
use crate::workspace::startup_hint::WorkspaceStartupHint;

use self::launcher::DaemonLauncher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForwardOutcome {
    SessionEnded,
    ImmediateDaemonDisconnect,
}

/// Run the adapter: auto-start daemon, connect, forward bytes.
///
/// This is the default entry point when no subcommand is given. It:
/// 1. Ensures the daemon is running (spawning it if necessary)
/// 2. Connects to the daemon's IPC socket
/// 3. Sends the workspace headers (`WORKSPACE:/path\n`, source, version)
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
pub async fn run_adapter(startup_hint: WorkspaceStartupHint) -> Result<()> {
    let paths = DaemonPaths::new();
    let launcher = DaemonLauncher::new(paths.clone());

    run_adapter_with(
        || {
            tokio::task::block_in_place(|| launcher.ensure_daemon_ready())
                .context("Failed to ensure daemon is ready")
        },
        || connect_and_handshake(&paths, &startup_hint),
        forward_bytes,
    )
    .await
}

pub(crate) async fn run_adapter_with<
    EnsureReady,
    Connect,
    ConnectFuture,
    Forward,
    ForwardFuture,
    Stream,
>(
    mut ensure_ready: EnsureReady,
    mut connect_and_handshake: Connect,
    mut forward: Forward,
) -> Result<()>
where
    EnsureReady: FnMut() -> Result<()>,
    Connect: FnMut() -> ConnectFuture,
    ConnectFuture: Future<Output = Result<Stream>>,
    Forward: FnMut(Stream) -> ForwardFuture,
    ForwardFuture: Future<Output = Result<ForwardOutcome>>,
{
    // Retry loop for initial connection and immediate restart handoffs. The
    // daemon may reject our connection before handshake, or it may accept the
    // connection, read our headers, decide it is stale, and close immediately
    // so a fresh daemon can be spawned from this binary.
    const MAX_RETRIES: u32 = 2;
    for attempt in 0..=MAX_RETRIES {
        ensure_ready()?;

        let stream = match connect_and_handshake().await {
            Ok(s) => s,
            Err(e) => {
                match restart_handoff_action(
                    attempt,
                    MAX_RETRIES,
                    RestartReason::TransportUnavailable,
                ) {
                    RestartHandoffAction::Retry { reason } => {
                        info!(
                            ?reason,
                            attempt = attempt + 1,
                            error = %e,
                            "Connection attempt failed during daemon restart handoff, retrying"
                        );
                        continue;
                    }
                    RestartHandoffAction::Exhausted { .. } => {
                        return Err(e).context("Failed to connect to daemon after retries");
                    }
                }
            }
        };

        info!("Adapter connected to daemon, forwarding bytes");

        match forward(stream).await {
            Ok(ForwardOutcome::SessionEnded) => {
                info!("Adapter session ended normally");
                return Ok(());
            }
            Ok(ForwardOutcome::ImmediateDaemonDisconnect) => {
                match restart_handoff_action(
                    attempt,
                    MAX_RETRIES,
                    RestartReason::ImmediateDisconnect,
                ) {
                    RestartHandoffAction::Retry { reason } => {
                        info!(
                            ?reason,
                            attempt = attempt + 1,
                            "Daemon closed immediately after handshake, retrying"
                        );
                        continue;
                    }
                    RestartHandoffAction::Exhausted { reason } => {
                        anyhow::bail!(
                            "Daemon closed immediately after handshake after {} attempts ({:?})",
                            MAX_RETRIES + 1,
                            reason
                        );
                    }
                }
            }
            Err(e) => {
                error!("Adapter connection lost: {}", e);
                // Exit cleanly; the MCP client will restart the adapter
                return Ok(());
            }
        }
    }

    unreachable!("retry loop either returns success or exits with an error")
}

/// Connect to the daemon IPC endpoint and send the workspace header.
async fn connect_and_handshake(
    paths: &DaemonPaths,
    startup_hint: &WorkspaceStartupHint,
) -> Result<IpcClientStream> {
    let ipc_addr = paths.daemon_ipc_addr();
    let mut stream = IpcConnector::connect(&ipc_addr)
        .await
        .context("Failed to connect to daemon IPC endpoint")?;

    // Send IPC headers: workspace path, then adapter version.
    // Path is sent as-is (native format); generate_workspace_id() normalizes
    // internally so the workspace ID is consistent regardless of separators.
    let header = build_ipc_header(startup_hint);
    stream
        .write_all(header.as_bytes())
        .await
        .context("Failed to send IPC headers")?;

    Ok(stream)
}

pub(crate) fn build_ipc_header(startup_hint: &WorkspaceStartupHint) -> String {
    let mut header = format!(
        "WORKSPACE:{}\nVERSION:{}\n",
        startup_hint.path.to_string_lossy(),
        env!("CARGO_PKG_VERSION"),
    );

    if let Some(source) = startup_hint.source {
        header.push_str(&format!("WORKSPACE_SOURCE:{}\n", source.as_header_value()));
    }

    header.push('\n');
    header
}

/// Bidirectional byte forwarding between stdin/stdout and the IPC stream.
///
/// Both directions run concurrently. When stdin closes (MCP client sent its
/// last message or exited), we shut down the IPC write side to signal the
/// daemon, then keep draining daemon-to-stdout until the daemon is done.
/// When stdout closes (client gone), we stop immediately.
async fn forward_bytes(stream: IpcClientStream) -> Result<ForwardOutcome> {
    let (mut ipc_read, mut ipc_write) = tokio::io::split(stream);
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    forward_streams_inner(&mut ipc_read, &mut ipc_write, &mut stdin, &mut stdout).await
}

#[cfg(test)]
pub(crate) async fn forward_streams<S, In, Out>(
    stream: S,
    stdin: &mut In,
    stdout: &mut Out,
) -> Result<ForwardOutcome>
where
    S: AsyncRead + AsyncWrite + Unpin,
    In: AsyncRead + Unpin,
    Out: AsyncWrite + Unpin,
{
    let (mut ipc_read, mut ipc_write) = tokio::io::split(stream);
    forward_streams_inner(&mut ipc_read, &mut ipc_write, stdin, stdout).await
}

async fn forward_streams_inner<IpcRead, IpcWrite, In, Out>(
    ipc_read: &mut IpcRead,
    ipc_write: &mut IpcWrite,
    stdin: &mut In,
    stdout: &mut Out,
) -> Result<ForwardOutcome>
where
    IpcRead: AsyncRead + Unpin,
    IpcWrite: AsyncWrite + Unpin,
    In: AsyncRead + Unpin,
    Out: AsyncWrite + Unpin,
{
    enum BranchOutcome {
        SessionEnded,
        ImmediateDaemonDisconnect,
    }

    // Run both directions concurrently with tokio::select!, but when
    // stdin->daemon finishes, don't return immediately. Instead, fall
    // through to drain the daemon->stdout direction.
    let outcome;
    tokio::select! {
        result = copy(stdin, ipc_write) => {
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
            outcome = match copy(ipc_read, stdout).await {
                Ok(bytes) => {
                    info!("daemon->stdout forwarding ended ({} bytes)", bytes);
                    BranchOutcome::SessionEnded
                }
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                    info!("stdout closed (client exited)");
                    BranchOutcome::SessionEnded
                }
                Err(e) => return Err(anyhow::anyhow!("daemon->stdout forwarding error: {}", e)),
            };
        }
        result = copy(ipc_read, stdout) => {
            // Daemon closed its side or stdout broke. Nothing more to do.
            outcome = match result {
                Ok(0) => BranchOutcome::ImmediateDaemonDisconnect,
                Ok(bytes) => {
                    info!("daemon->stdout forwarding ended ({} bytes)", bytes);
                    BranchOutcome::SessionEnded
                }
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                    info!("stdout closed (client exited)");
                    BranchOutcome::SessionEnded
                }
                Err(e) => return Err(anyhow::anyhow!("daemon->stdout forwarding error: {}", e)),
            };
        }
    }

    match outcome {
        BranchOutcome::SessionEnded => Ok(ForwardOutcome::SessionEnded),
        BranchOutcome::ImmediateDaemonDisconnect => {
            info!("daemon closed immediately after handshake");
            Ok(ForwardOutcome::ImmediateDaemonDisconnect)
        }
    }
}
