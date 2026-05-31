use std::collections::VecDeque;

use anyhow::{Context, Result};
use rmcp::model::{ClientJsonRpcMessage, JsonRpcMessage, RequestId, ServerJsonRpcMessage};
use rmcp::service::RoleClient;
use rmcp::transport::Transport;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::adapter::ForwardOutcome;

/// Error variants returned by the HTTP adapter forwarder.
///
/// Distinguishes daemon-side transport failures from MCP-client side I/O failures.
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
        /// failed. Present when `send_client_line` fails after parsing but
        /// before the line reaches `in_flight_requests`.
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

pub(super) fn spawn_stdin_line_reader<In>(
    stdin: In,
) -> mpsc::Receiver<Result<Vec<u8>, std::io::Error>>
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

pub(super) async fn forward_http_stdio_transport_with_pending<T, Out>(
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
                return Err(handle_send_failure(
                    error,
                    lost_line,
                    wrote_any_output,
                    stdout,
                    pending_lines,
                    &mut in_flight_requests,
                )
                .await);
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
                            return Err(handle_send_failure(
                                error,
                                lost_line,
                                wrote_any_output,
                                stdout,
                                pending_lines,
                                &mut in_flight_requests,
                            )
                            .await);
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
                        return Ok(ForwardOutcome::ImmediateDaemonDisconnect);
                    }
                    // Output was already written this session, so it cannot be
                    // safely replayed against a fresh daemon. Answer every
                    // still-in-flight request with an error so the MCP client
                    // fails those calls instead of hanging forever waiting for
                    // a reply that will never arrive.
                    answer_orphaned_in_flight(stdout, &in_flight_requests).await;
                    return Err(AdapterError::Transport {
                        error: anyhow::anyhow!(
                            "HTTP MCP daemon closed with {} request(s) still in flight after output was written",
                            in_flight_requests.len()
                        ),
                        wrote_any_output,
                        lost_line: None,
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
                        Err(other) => {
                            std::io::Error::new(std::io::ErrorKind::Other, other.to_string())
                        }
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
/// On success the line is moved into `in_flight_requests` if it carries a
/// request id. On transport failure the raw bytes are returned so the caller can
/// push them back onto `pending_lines`.
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

    let message: ClientJsonRpcMessage = match serde_json::from_slice(&line) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("Ignoring malformed MCP client message: {e}");
            return Ok(());
        }
    };
    let expected_response_id = client_request_id(&message);

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

/// Resolve a transport `send` failure into an `AdapterError`, answering the
/// client where the session can no longer be safely retried.
///
/// * Before any output (`wrote_any_output == false`): the whole session can be
///   replayed against a fresh daemon, so requeue the in-flight requests and let
///   the retry loop resend them. The `lost_line` is returned in the error and
///   pushed back onto `pending_lines` by the retry path.
/// * After output (`wrote_any_output == true`): replay could double-apply
///   non-idempotent tools, so the adapter will exit instead. Answer every
///   request the client is still waiting on — the already-sent in-flight ones
///   plus the request whose send just failed — with a synthesized error so the
///   client fails those calls rather than hanging forever.
async fn handle_send_failure<Out>(
    error: anyhow::Error,
    lost_line: Vec<u8>,
    wrote_any_output: bool,
    stdout: &mut Out,
    pending_lines: &mut VecDeque<Vec<u8>>,
    in_flight_requests: &mut VecDeque<(RequestId, Vec<u8>)>,
) -> AdapterError
where
    Out: AsyncWrite + Unpin,
{
    if !wrote_any_output {
        requeue_in_flight(pending_lines, in_flight_requests);
    } else {
        answer_orphaned_in_flight(stdout, in_flight_requests).await;
        if let Some(id) = request_id_from_line(&lost_line) {
            write_lost_connection_error(stdout, &id).await;
        }
    }
    AdapterError::Transport {
        error,
        wrote_any_output,
        lost_line: Some(lost_line),
    }
}

/// Synthesize a JSON-RPC error response for every request that was still in
/// flight when the daemon connection was lost, and write each to the MCP
/// client's stdout.
///
/// Without this, the MCP client (e.g. Claude Code) stays blocked forever on the
/// pending tool call — the all-night hang. Writing an error response for each
/// orphaned request id lets the client fail the call cleanly instead.
pub(super) async fn answer_orphaned_in_flight<Out>(
    stdout: &mut Out,
    in_flight_requests: &VecDeque<(RequestId, Vec<u8>)>,
) where
    Out: AsyncWrite + Unpin,
{
    for (request_id, _) in in_flight_requests {
        write_lost_connection_error(stdout, request_id).await;
    }
}

/// Write a single synthesized "daemon connection lost" JSON-RPC error response
/// for `request_id`. Best-effort: if the client's stdout is already gone there
/// is nothing more to do, so write/flush errors are ignored.
pub(super) async fn write_lost_connection_error<Out>(stdout: &mut Out, request_id: &RequestId)
where
    Out: AsyncWrite + Unpin,
{
    let Ok(id) = serde_json::to_value(request_id) else {
        return;
    };
    let message = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": -32603,
            "message": "Julie daemon connection lost before the request completed (the daemon may be restarting); please retry."
        }
    });
    let Ok(mut bytes) = serde_json::to_vec(&message) else {
        return;
    };
    bytes.push(b'\n');
    let _ = stdout.write_all(&bytes).await;
    let _ = stdout.flush().await;
}

/// Extract the JSON-RPC request id from a raw client line, if it carries one.
/// Used to answer un-replayed requests still sitting in `pending_lines` when the
/// adapter exhausts its retry budget.
pub(super) fn request_id_from_line(line: &[u8]) -> Option<RequestId> {
    let message: ClientJsonRpcMessage = serde_json::from_slice(line).ok()?;
    client_request_id(&message)
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
