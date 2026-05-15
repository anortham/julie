use std::collections::VecDeque;
use std::time::Duration;

use anyhow::{Context, Result};
use rmcp::model::{ClientJsonRpcMessage, JsonRpcMessage, RequestId, ServerJsonRpcMessage};
use rmcp::service::RoleClient;
use rmcp::transport::{StreamableHttpClientTransport, Transport};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::adapter::ForwardOutcome;
use crate::adapter::launcher::DaemonLauncher;
use crate::daemon::http_client::http_client_config_for_endpoint;
use crate::daemon::lifecycle::{RestartHandoffAction, RestartReason, restart_handoff_action};
use crate::workspace::startup_hint::WorkspaceStartupHint;

/// Error variants returned by the HTTP adapter forwarder.
///
/// Distinguishes daemon-side transport failures (potentially recoverable by
/// retry/respawn) from MCP-client side I/O failures (terminal).
#[derive(Debug)]
pub(crate) enum AdapterError {
    /// Failure while talking to the daemon over HTTP MCP. `wrote_any_output`
    /// captures whether any server response has already been forwarded to the
    /// MCP client; if so, replaying the session would risk double-applying
    /// non-idempotent tool calls.
    Transport {
        error: anyhow::Error,
        wrote_any_output: bool,
        /// Raw request line bytes that were being sent when the transport
        /// failed.  Present when `send_client_line` fails *after* parsing
        /// but *before* the line reaches `in_flight_requests`.  The retry
        /// loop in `run_http_adapter` pushes this back onto
        /// `pending_lines` so the request is not silently dropped.
        lost_line: Option<Vec<u8>>,
    },
    /// Failure reading from MCP client stdin or writing to its stdout. The
    /// MCP client is gone; retrying makes no sense.
    Stdin(std::io::Error),
}

impl std::fmt::Display for AdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AdapterError::Transport { error, .. } => write!(f, "{}", error),
            AdapterError::Stdin(error) => write!(f, "MCP client stdio error: {}", error),
        }
    }
}

impl std::error::Error for AdapterError {}

/// Outcome of classifying an AdapterError for the retry loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdapterRetryDecision {
    /// Daemon transport error before any output; retry against a fresh daemon.
    Retry,
    /// Stdin error, or transport error after output was already written.
    /// Mid-session retry would require a new MCP handshake and could
    /// double-apply non-idempotent tools, so we exit cleanly.
    Terminal,
    /// Retry budget exhausted.
    Exhausted,
}

/// Decide whether to retry, exit cleanly, or fail after exhausting retries.
pub(crate) fn classify_adapter_error(
    error: &AdapterError,
    attempt: u32,
    max_retries: u32,
) -> AdapterRetryDecision {
    match error {
        AdapterError::Stdin(_) => AdapterRetryDecision::Terminal,
        AdapterError::Transport {
            wrote_any_output: true,
            ..
        } => AdapterRetryDecision::Terminal,
        AdapterError::Transport {
            wrote_any_output: false,
            ..
        } => match restart_handoff_action(attempt, max_retries, RestartReason::ImmediateDisconnect)
        {
            RestartHandoffAction::Retry { .. } => AdapterRetryDecision::Retry,
            RestartHandoffAction::Exhausted { .. } => AdapterRetryDecision::Exhausted,
        },
    }
}

/// Maximum number of retry attempts after the first connect.
///
/// Combined with exponential backoff (1s, 2s, 4s, 8s, 16s) this yields a
/// total retry window of ~31s, which fits within the daemon drain timeout.
pub(crate) const MAX_RETRIES: u32 = 5;

/// Compute the backoff to apply before the given retry attempt.
///
/// `attempt` is the 1-based retry attempt number (the first retry is 1).
/// Returns 1s, 2s, 4s, 8s, 16s, ... clamped at 16s.
pub(crate) fn retry_backoff(attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(4);
    Duration::from_secs(1u64 << shift)
}

pub(crate) async fn run_http_adapter(
    startup_hint: WorkspaceStartupHint,
    launcher: DaemonLauncher,
) -> Result<()> {
    let mut stdin_lines = spawn_stdin_line_reader(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut pending_lines = VecDeque::new();

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let backoff = retry_backoff(attempt);
            info!(
                attempt = attempt + 1,
                backoff_secs = backoff.as_secs(),
                "HTTP adapter backing off before retry"
            );
            tokio::time::sleep(backoff).await;
        }

        tokio::task::block_in_place(|| launcher.ensure_daemon_ready())
            .context("Failed to ensure daemon is ready")?;

        let endpoint = match launcher.transport_endpoint() {
            Ok(endpoint) => endpoint,
            Err(error) => {
                match restart_handoff_action(
                    attempt,
                    MAX_RETRIES,
                    RestartReason::TransportUnavailable,
                ) {
                    RestartHandoffAction::Retry { reason } => {
                        info!(
                            ?reason,
                            attempt = attempt + 1,
                            error = %error,
                            "HTTP adapter transport discovery failed during daemon restart handoff, retrying"
                        );
                        continue;
                    }
                    RestartHandoffAction::Exhausted { .. } => {
                        return Err(anyhow::Error::from(error))
                            .context("Failed to discover HTTP daemon transport after retries");
                    }
                }
            }
        };

        let config = match http_client_config_for_endpoint(&endpoint, &startup_hint) {
            Ok(config) => config,
            Err(error) => {
                match restart_handoff_action(
                    attempt,
                    MAX_RETRIES,
                    RestartReason::TransportUnavailable,
                ) {
                    RestartHandoffAction::Retry { reason } => {
                        info!(
                            ?reason,
                            attempt = attempt + 1,
                            error = %error,
                            "HTTP adapter transport discovery failed during daemon restart handoff, retrying"
                        );
                        continue;
                    }
                    RestartHandoffAction::Exhausted { .. } => {
                        return Err(error)
                            .context("Failed to discover HTTP daemon transport after retries");
                    }
                }
            }
        };

        let transport = StreamableHttpClientTransport::from_config(config);
        match forward_http_stdio_transport_with_pending(
            transport,
            &mut stdin_lines,
            &mut stdout,
            &mut pending_lines,
        )
        .await
        {
            Ok(ForwardOutcome::SessionEnded) => {
                info!("HTTP adapter session ended normally");
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
                            "HTTP daemon closed before a response, retrying"
                        );
                        continue;
                    }
                    RestartHandoffAction::Exhausted { reason } => {
                        anyhow::bail!(
                            "HTTP daemon closed before a response after {} attempts ({:?})",
                            MAX_RETRIES + 1,
                            reason
                        );
                    }
                }
            }
            Err(adapter_error) => match classify_adapter_error(
                &adapter_error,
                attempt,
                MAX_RETRIES,
            ) {
                AdapterRetryDecision::Retry => {
                    info!(
                        attempt = attempt + 1,
                        error = %adapter_error,
                        "HTTP adapter transport error before output, retrying"
                    );
                    // Recover the request line that was in-flight when the
                    // transport failed so it is retried on the next attempt.
                    if let AdapterError::Transport { lost_line: Some(line), .. } = adapter_error {
                        pending_lines.push_front(line);
                    }
                    continue;
                }
                AdapterRetryDecision::Terminal => {
                    match adapter_error {
                        AdapterError::Stdin(error) => {
                            info!("HTTP adapter MCP client stdio closed: {}", error);
                        }
                        AdapterError::Transport { error, .. } => {
                            error!(
                                "HTTP adapter transport error after output written, exiting: {}",
                                error
                            );
                        }
                    }
                    return Ok(());
                }
                AdapterRetryDecision::Exhausted => match adapter_error {
                    AdapterError::Transport { error, .. } => {
                        return Err(error).context(format!(
                            "HTTP adapter transport error before output after {} attempts",
                            MAX_RETRIES + 1
                        ));
                    }
                    AdapterError::Stdin(error) => {
                        // Stdin errors are classified as Terminal above; reaching
                        // Exhausted with a Stdin error indicates a logic bug.
                        return Err(anyhow::Error::from(error))
                            .context("Unexpected exhausted decision for stdin error");
                    }
                },
            },
        }
    }

    unreachable!("retry loop either returns success or exits with an error")
}

#[cfg(test)]
pub(crate) async fn forward_http_stdio_transport<T, In, Out>(
    transport: T,
    stdin: In,
    stdout: &mut Out,
) -> Result<ForwardOutcome, AdapterError>
where
    T: Transport<RoleClient>,
    In: AsyncRead + Send + Unpin + 'static,
    Out: AsyncWrite + Unpin,
{
    let mut stdin_lines = spawn_stdin_line_reader(stdin);
    let mut pending_lines = VecDeque::new();
    forward_http_stdio_transport_with_pending(
        transport,
        &mut stdin_lines,
        stdout,
        &mut pending_lines,
    )
    .await
}

fn spawn_stdin_line_reader<In>(stdin: In) -> mpsc::Receiver<Result<Vec<u8>, std::io::Error>>
where
    In: AsyncRead + Send + Unpin + 'static,
{
    let (tx, rx) = mpsc::channel(16);
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdin);
        loop {
            let mut line = Vec::new();
            match reader.read_until(b'\n', &mut line).await {
                Ok(0) => break,
                Ok(_) => {
                    if tx.send(Ok(line)).await.is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = tx.send(Err(error)).await;
                    break;
                }
            }
        }
    });
    rx
}

async fn forward_http_stdio_transport_with_pending<T, Out>(
    mut transport: T,
    stdin_lines: &mut mpsc::Receiver<Result<Vec<u8>, std::io::Error>>,
    stdout: &mut Out,
    pending_lines: &mut VecDeque<Vec<u8>>,
) -> Result<ForwardOutcome, AdapterError>
where
    T: Transport<RoleClient>,
    Out: AsyncWrite + Unpin,
{
    let mut wrote_any_output = false;
    let mut stdin_done = false;
    let mut in_flight_requests: VecDeque<(RequestId, Vec<u8>)> = VecDeque::new();

    loop {
        if let Some(line) = pending_lines.pop_front() {
            if let Err((error, lost_line)) =
                send_client_line(&mut transport, line, &mut in_flight_requests).await
            {
                if !wrote_any_output {
                    requeue_in_flight(pending_lines, &mut in_flight_requests);
                }
                return Err(AdapterError::Transport {
                    error,
                    wrote_any_output,
                    lost_line: Some(lost_line),
                });
            }
            continue;
        }

        if stdin_done && in_flight_requests.is_empty() {
            if let Err(error) = transport
                .close()
                .await
                .context("Failed to close HTTP MCP transport")
            {
                return Err(AdapterError::Transport {
                    error,
                    wrote_any_output,
                    lost_line: None,
                });
            }
            return Ok(ForwardOutcome::SessionEnded);
        }

        tokio::select! {
            line = stdin_lines.recv(), if !stdin_done => {
                match line {
                    Some(Ok(line)) => {
                        if let Err((error, lost_line)) = send_client_line(&mut transport, line, &mut in_flight_requests).await {
                            if !wrote_any_output {
                                requeue_in_flight(pending_lines, &mut in_flight_requests);
                            }
                            return Err(AdapterError::Transport {
                                error,
                                wrote_any_output,
                                lost_line: Some(lost_line),
                            });
                        }
                    }
                    Some(Err(error)) => {
                        return Err(AdapterError::Stdin(error));
                    }
                    None => {
                        stdin_done = true;
                    }
                }
            }
            response = transport.receive(), if !in_flight_requests.is_empty() => {
                let Some(response) = response else {
                    if !wrote_any_output {
                        requeue_in_flight(pending_lines, &mut in_flight_requests);
                    }
                    return Ok(if wrote_any_output {
                        ForwardOutcome::SessionEnded
                    } else {
                        ForwardOutcome::ImmediateDaemonDisconnect
                    });
                };

                if !wrote_any_output
                    && is_restart_required_response_for_in_flight(
                        &response,
                        &in_flight_requests,
                    )
                {
                    requeue_in_flight(pending_lines, &mut in_flight_requests);
                    return Ok(ForwardOutcome::ImmediateDaemonDisconnect);
                }

                remove_completed_request(&mut in_flight_requests, &response);
                if let Err(error) = write_server_message(stdout, &response).await {
                    let io_error = match error.downcast::<std::io::Error>() {
                        Ok(io) => io,
                        Err(other) => std::io::Error::new(std::io::ErrorKind::Other, other.to_string()),
                    };
                    return Err(AdapterError::Stdin(io_error));
                }
                wrote_any_output = true;
            }
        }
    }
}

/// Send a parsed JSON-RPC line to the transport and track it for retry.
///
/// On success the line is moved into `in_flight_requests` (if it carries a
/// request id).  On transport failure the raw bytes are returned inside the
/// error tuple so the caller can push them back onto `pending_lines`.
async fn send_client_line<T>(
    transport: &mut T,
    line: Vec<u8>,
    in_flight_requests: &mut VecDeque<(RequestId, Vec<u8>)>,
) -> Result<(), (anyhow::Error, Vec<u8>)>
where
    T: Transport<RoleClient>,
{
    if line.iter().all(u8::is_ascii_whitespace) {
        return Ok(());
    }

    let message: ClientJsonRpcMessage = serde_json::from_slice(&line)
        .context("Failed to parse MCP client JSON-RPC message")
        .map_err(|e| (e, line.clone()))?;
    let expected_response_id = client_request_id(&message);

    // `transport.send()` takes ownership of `message`, not `line`, so
    // `line` is still available on the error path without cloning.
    if let Err(send_err) = transport
        .send(message)
        .await
        .context("Failed to send JSON-RPC message to HTTP MCP daemon")
    {
        return Err((send_err, line));
    }

    if let Some(expected_response_id) = expected_response_id {
        in_flight_requests.push_back((expected_response_id, line));
    }

    Ok(())
}

fn requeue_in_flight(
    pending_lines: &mut VecDeque<Vec<u8>>,
    in_flight_requests: &mut VecDeque<(RequestId, Vec<u8>)>,
) {
    while let Some((_, line)) = in_flight_requests.pop_back() {
        pending_lines.push_front(line);
    }
}

fn remove_completed_request(
    in_flight_requests: &mut VecDeque<(RequestId, Vec<u8>)>,
    message: &ServerJsonRpcMessage,
) {
    let Some(id) = server_response_id(message) else {
        return;
    };
    if let Some(index) = in_flight_requests
        .iter()
        .position(|(pending_id, _)| pending_id == id)
    {
        in_flight_requests.remove(index);
    }
}

fn is_restart_required_response_for_in_flight(
    message: &ServerJsonRpcMessage,
    in_flight_requests: &VecDeque<(RequestId, Vec<u8>)>,
) -> bool {
    match message {
        JsonRpcMessage::Error(error) => {
            error.error.code.0 == -32603
                && error.error.message.contains("restart")
                && in_flight_requests
                    .iter()
                    .any(|(request_id, _)| request_id == &error.id)
        }
        _ => false,
    }
}

fn server_response_id(message: &ServerJsonRpcMessage) -> Option<&RequestId> {
    match message {
        JsonRpcMessage::Response(response) => Some(&response.id),
        JsonRpcMessage::Error(error) => Some(&error.id),
        _ => None,
    }
}

fn client_request_id(message: &ClientJsonRpcMessage) -> Option<RequestId> {
    match message {
        JsonRpcMessage::Request(request) => Some(request.id.clone()),
        _ => None,
    }
}

async fn write_server_message<Out>(stdout: &mut Out, message: &ServerJsonRpcMessage) -> Result<()>
where
    Out: AsyncWrite + Unpin,
{
    let mut encoded = serde_json::to_vec(message).context("Failed to serialize daemon response")?;
    encoded.push(b'\n');
    stdout
        .write_all(&encoded)
        .await
        .context("Failed to write MCP response to stdout")?;
    stdout
        .flush()
        .await
        .context("Failed to flush MCP response to stdout")
}
