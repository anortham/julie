use std::collections::{HashMap, VecDeque};

use anyhow::{Context, Result};
use axum::http::{HeaderName, HeaderValue};
use rmcp::model::{ClientJsonRpcMessage, JsonRpcMessage, RequestId, ServerJsonRpcMessage};
use rmcp::service::RoleClient;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{StreamableHttpClientTransport, Transport};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::adapter::ForwardOutcome;
use crate::adapter::launcher::DaemonLauncher;
use crate::daemon::lifecycle::{RestartHandoffAction, RestartReason, restart_handoff_action};
use crate::daemon::mcp_session::{
    HEADER_JULIE_VERSION, HEADER_JULIE_WORKSPACE, HEADER_JULIE_WORKSPACE_SOURCE,
};
use crate::daemon::transport::TransportEndpoint;
use crate::workspace::startup_hint::WorkspaceStartupHint;

pub(crate) async fn run_http_adapter(
    startup_hint: WorkspaceStartupHint,
    launcher: DaemonLauncher,
) -> Result<()> {
    const MAX_RETRIES: u32 = 2;

    let mut stdin_lines = spawn_stdin_line_reader(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut pending_lines = VecDeque::new();

    for attempt in 0..=MAX_RETRIES {
        tokio::task::block_in_place(|| launcher.ensure_daemon_ready())
            .context("Failed to ensure daemon is ready")?;

        let config = match http_client_config_for_endpoint(
            &launcher.transport_endpoint(),
            &startup_hint,
        ) {
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
            Err(error) => {
                error!("HTTP adapter connection lost: {}", error);
                return Ok(());
            }
        }
    }

    unreachable!("retry loop either returns success or exits with an error")
}

pub(crate) fn http_client_config_for_endpoint(
    endpoint: &TransportEndpoint,
    startup_hint: &WorkspaceStartupHint,
) -> Result<StreamableHttpClientTransportConfig> {
    let uri = endpoint
        .mcp_url()
        .context("daemon transport discovery did not contain an HTTP MCP URL")?;
    let mut headers = HashMap::new();
    let workspace_path = startup_hint.path.to_string_lossy();
    headers.insert(
        HeaderName::from_static(HEADER_JULIE_WORKSPACE),
        HeaderValue::from_str(workspace_path.as_ref())
            .context("workspace path is not valid as an HTTP header")?,
    );
    if let Some(source) = startup_hint.source {
        headers.insert(
            HeaderName::from_static(HEADER_JULIE_WORKSPACE_SOURCE),
            HeaderValue::from_static(source.as_header_value()),
        );
    }
    headers.insert(
        HeaderName::from_static(HEADER_JULIE_VERSION),
        HeaderValue::from_static(env!("CARGO_PKG_VERSION")),
    );

    let mut config = StreamableHttpClientTransportConfig::with_uri(uri).custom_headers(headers);
    if let Some(token_path) = endpoint.token_path() {
        let token = std::fs::read_to_string(token_path).with_context(|| {
            format!("Failed to read HTTP MCP token at {}", token_path.display())
        })?;
        let token = token.trim();
        if token.is_empty() || token.contains('\r') || token.contains('\n') {
            anyhow::bail!("HTTP MCP token file is empty or malformed");
        }
        config = config.auth_header(token.to_string());
    }
    Ok(config)
}

#[cfg(test)]
pub(crate) async fn forward_http_stdio_transport<T, In, Out>(
    transport: T,
    stdin: In,
    stdout: &mut Out,
) -> Result<ForwardOutcome>
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
) -> Result<ForwardOutcome>
where
    T: Transport<RoleClient>,
    Out: AsyncWrite + Unpin,
{
    let mut wrote_any_output = false;
    let mut stdin_done = false;
    let mut in_flight_requests: VecDeque<(RequestId, Vec<u8>)> = VecDeque::new();

    loop {
        if let Some(line) = pending_lines.pop_front() {
            send_client_line(&mut transport, line, &mut in_flight_requests).await?;
            continue;
        }

        if stdin_done && in_flight_requests.is_empty() {
            transport
                .close()
                .await
                .context("Failed to close HTTP MCP transport")?;
            return Ok(ForwardOutcome::SessionEnded);
        }

        tokio::select! {
            line = stdin_lines.recv(), if !stdin_done => {
                match line {
                    Some(Ok(line)) => {
                        send_client_line(&mut transport, line, &mut in_flight_requests).await?;
                    }
                    Some(Err(error)) => {
                        return Err(anyhow::Error::from(error))
                            .context("Failed to read MCP client stdin");
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
                write_server_message(stdout, &response).await?;
                wrote_any_output = true;
            }
        }
    }
}

async fn send_client_line<T>(
    transport: &mut T,
    line: Vec<u8>,
    in_flight_requests: &mut VecDeque<(RequestId, Vec<u8>)>,
) -> Result<()>
where
    T: Transport<RoleClient>,
{
    if line.iter().all(u8::is_ascii_whitespace) {
        return Ok(());
    }

    let message: ClientJsonRpcMessage =
        serde_json::from_slice(&line).context("Failed to parse MCP client JSON-RPC message")?;
    let expected_response_id = client_request_id(&message);
    transport
        .send(message)
        .await
        .context("Failed to send JSON-RPC message to HTTP MCP daemon")?;

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
