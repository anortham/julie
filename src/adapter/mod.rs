//! Adapter: the thin process that MCP clients spawn.
//!
//! The adapter auto-starts the daemon if not running, connects to the IPC
//! socket, sends a workspace header, then bidirectionally forwards bytes
//! between stdin/stdout and the daemon. From the MCP client's perspective,
//! it looks exactly like a stdio MCP server.

pub mod launcher;

use anyhow::{Context, Result};
use std::future::Future;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, copy};
use tracing::{error, info, warn};

use crate::daemon::ipc::{DAEMON_READY_LINE, IpcClientStream, IpcConnector};
use crate::daemon::lifecycle::{RestartHandoffAction, RestartReason, restart_handoff_action};
use crate::paths::DaemonPaths;
use crate::workspace::startup_hint::WorkspaceStartupHint;

use self::launcher::DaemonLauncher;

/// How long the adapter waits for the daemon's ready signal after sending
/// headers. The daemon writes the signal after clearing all accept-path
/// gates but before calling rmcp's `serve`, which for a non-deferred
/// startup hint sits behind `WorkspacePool::get_or_init` — i.e. DB open,
/// file scan, and (cold) index build. On a large workspace that work can
/// take tens of seconds, so the budget has to be generous. 30 s matches
/// the MCP client's typical `initialize` timeout: if the daemon can't
/// produce READY inside that window, the client is going to give up on
/// the session anyway, so treating it as a hard failure here is correct.
const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// Safety cap on the length of a single protocol line the adapter will buffer
/// while waiting for the ready signal. The real signal is 13 bytes; 64 gives
/// room for future signals without exposing the adapter to runaway allocation
/// if the daemon misbehaves.
const READY_LINE_MAX: usize = 64;

/// Outcome of attempting to read the daemon's ready signal.
#[derive(Debug)]
pub(crate) enum ReadyOutcome {
    /// Daemon wrote `DAEMON_READY\n`. Safe to start forwarding.
    Ready,
    /// Daemon closed the stream before sending the ready signal. The caller
    /// should treat this like an immediate disconnect and retry.
    Eof,
    /// Timed out waiting for any newline-terminated line. Most likely a
    /// legacy daemon that predates the handshake protocol. The caller should
    /// log a warning and fall back to legacy forwarding.
    Timeout,
    /// A complete line arrived that wasn't the ready signal. The caller
    /// should treat this as a protocol error and retry.
    Unexpected(Vec<u8>),
    /// Underlying I/O error while reading.
    IoError(std::io::Error),
}

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

/// Connect to the daemon IPC endpoint, send the workspace header, and wait
/// for the daemon's ready signal before returning.
///
/// The ready signal closes a byte-loss race that could otherwise happen when
/// the daemon decides to drop the connection at an accept-path gate (stale
/// binary, version mismatch): without this ordering, the adapter's forwarder
/// might read and discard client stdin before the retry reconnects.
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

    // All non-Ready outcomes are retryable transport failures: the stream is
    // dropped, run_adapter_with's retry loop reconnects, and client stdin is
    // still untouched (this is the whole point of the handshake). There is no
    // "legacy fallback" — an older daemon that doesn't know the READY
    // protocol will look like a timeout or unexpected-line, which is
    // symptomatically correct: the client sees a failed attempt, the adapter
    // bumps the retry counter, and when the retry either reaches a new
    // daemon or exhausts, the user gets an actionable failure instead of a
    // silent byte-loss race.
    match read_daemon_ready(&mut stream, READY_TIMEOUT).await {
        ReadyOutcome::Ready => Ok(stream),
        ReadyOutcome::Eof => anyhow::bail!(
            "Daemon closed connection before sending ready signal (accept-path rejection or pre-serve failure)"
        ),
        ReadyOutcome::Timeout => {
            warn!(
                "Daemon did not send ready signal within {:?}; treating as transport failure",
                READY_TIMEOUT
            );
            anyhow::bail!(
                "Daemon did not send ready signal within {:?}",
                READY_TIMEOUT
            )
        }
        ReadyOutcome::Unexpected(line) => anyhow::bail!(
            "Daemon sent unexpected pre-serve line: {:?}",
            String::from_utf8_lossy(&line)
        ),
        ReadyOutcome::IoError(e) => {
            Err(anyhow::Error::from(e)).context("I/O error while waiting for daemon ready signal")
        }
    }
}

/// Read the daemon's single-line ready signal from the stream.
///
/// The caller promises to drive this BEFORE any stdin forwarding starts.
/// Reads one byte at a time so that bytes following the terminating `\n`
/// (e.g. the daemon's first MCP response) remain in the stream for the
/// downstream byte-forwarder to pick up.
pub(crate) async fn read_daemon_ready<S>(stream: &mut S, timeout: Duration) -> ReadyOutcome
where
    S: AsyncRead + Unpin,
{
    let mut buf: Vec<u8> = Vec::with_capacity(DAEMON_READY_LINE.len());
    let mut byte = [0u8; 1];

    let read_line = async {
        loop {
            match stream.read(&mut byte).await {
                Ok(0) => return ReadyOutcome::Eof,
                Ok(_) => {
                    buf.push(byte[0]);
                    if byte[0] == b'\n' {
                        if buf == DAEMON_READY_LINE {
                            return ReadyOutcome::Ready;
                        }
                        return ReadyOutcome::Unexpected(std::mem::take(&mut buf));
                    }
                    if buf.len() >= READY_LINE_MAX {
                        return ReadyOutcome::Unexpected(std::mem::take(&mut buf));
                    }
                }
                Err(e) => return ReadyOutcome::IoError(e),
            }
        }
    };

    match tokio::time::timeout(timeout, read_line).await {
        Ok(outcome) => outcome,
        Err(_) => ReadyOutcome::Timeout,
    }
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
