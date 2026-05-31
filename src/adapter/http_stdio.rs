use std::collections::VecDeque;
use std::time::Duration;

use anyhow::{Context, Result};
use rmcp::service::RoleClient;
use rmcp::transport::{StreamableHttpClientTransport, Transport};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{error, info};

use crate::adapter::ForwardOutcome;
use crate::adapter::forwarder::{
    AdapterError, forward_http_stdio_transport_with_pending, request_id_from_line,
    spawn_stdin_line_reader, write_lost_connection_error,
};
use crate::adapter::launcher::DaemonLauncher;
use crate::daemon::http_client::http_client_config_for_endpoint;
use crate::daemon::lifecycle::{RestartHandoffAction, RestartReason, restart_handoff_action};
use crate::daemon::transport::TransportEndpoint;
use crate::workspace::startup_hint::WorkspaceStartupHint;

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

pub(crate) trait DaemonAdapterControl {
    fn ensure_daemon_ready(&self) -> std::io::Result<()>;
    fn transport_endpoint(&self) -> std::io::Result<TransportEndpoint>;
}

impl DaemonAdapterControl for DaemonLauncher {
    fn ensure_daemon_ready(&self) -> std::io::Result<()> {
        DaemonLauncher::ensure_daemon_ready(self)
    }

    fn transport_endpoint(&self) -> std::io::Result<TransportEndpoint> {
        DaemonLauncher::transport_endpoint(self)
    }
}

pub(crate) async fn run_http_adapter(
    startup_hint: WorkspaceStartupHint,
    launcher: DaemonLauncher,
) -> Result<()> {
    let mut stdout = tokio::io::stdout();
    run_http_adapter_inner(
        startup_hint,
        &launcher,
        tokio::io::stdin(),
        &mut stdout,
        |endpoint, startup_hint| {
            let config = http_client_config_for_endpoint(endpoint, startup_hint)?;
            Ok(StreamableHttpClientTransport::from_config(config))
        },
    )
    .await
}

pub(crate) async fn run_http_adapter_inner<C, F, T, In, Out>(
    startup_hint: WorkspaceStartupHint,
    launcher: &C,
    stdin: In,
    stdout: &mut Out,
    make_transport: F,
) -> Result<()>
where
    C: DaemonAdapterControl,
    F: FnMut(&TransportEndpoint, &WorkspaceStartupHint) -> Result<T>,
    T: Transport<RoleClient>,
    In: AsyncRead + Send + Unpin + 'static,
    Out: AsyncWrite + Unpin,
{
    run_http_adapter_inner_with_backoff(
        startup_hint,
        launcher,
        stdin,
        stdout,
        make_transport,
        retry_backoff,
    )
    .await
}

/// Backoff-injectable core of the retry loop. Production uses [`retry_backoff`];
/// tests pass a zero-delay closure so the full retry budget elapses instantly.
pub(crate) async fn run_http_adapter_inner_with_backoff<C, F, T, In, Out, B>(
    startup_hint: WorkspaceStartupHint,
    launcher: &C,
    stdin: In,
    stdout: &mut Out,
    mut make_transport: F,
    backoff: B,
) -> Result<()>
where
    C: DaemonAdapterControl,
    F: FnMut(&TransportEndpoint, &WorkspaceStartupHint) -> Result<T>,
    T: Transport<RoleClient>,
    In: AsyncRead + Send + Unpin + 'static,
    Out: AsyncWrite + Unpin,
    B: Fn(u32) -> Duration,
{
    let mut stdin_lines = spawn_stdin_line_reader(stdin);
    let mut pending_lines = VecDeque::new();

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            let backoff_delay = backoff(attempt);
            info!(
                attempt = attempt + 1,
                backoff_secs = backoff_delay.as_secs(),
                "HTTP adapter backing off before retry"
            );
            tokio::time::sleep(backoff_delay).await;
        }

        ensure_daemon_ready_blocking(launcher).context("Failed to ensure daemon is ready")?;

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
                        answer_unanswered_pending(stdout, &pending_lines).await;
                        return Err(anyhow::Error::from(error))
                            .context("Failed to discover HTTP daemon transport after retries");
                    }
                }
            }
        };

        let transport = match make_transport(&endpoint, &startup_hint) {
            Ok(transport) => transport,
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
                        answer_unanswered_pending(stdout, &pending_lines).await;
                        return Err(error)
                            .context("Failed to discover HTTP daemon transport after retries");
                    }
                }
            }
        };
        match forward_http_stdio_transport_with_pending(
            transport,
            &mut stdin_lines,
            stdout,
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
                        answer_unanswered_pending(stdout, &pending_lines).await;
                        anyhow::bail!(
                            "HTTP daemon closed before a response after {} attempts ({:?})",
                            MAX_RETRIES + 1,
                            reason
                        );
                    }
                }
            }
            Err(adapter_error) => {
                match classify_adapter_error(&adapter_error, attempt, MAX_RETRIES) {
                    AdapterRetryDecision::Retry => {
                        info!(
                            attempt = attempt + 1,
                            error = %adapter_error,
                            "HTTP adapter transport error before output, retrying"
                        );
                        // Recover the request line that was in-flight when the
                        // transport failed so it is retried on the next attempt.
                        if let AdapterError::Transport {
                            lost_line: Some(line),
                            ..
                        } = adapter_error
                        {
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
                                // The forwarder already answered the in-flight
                                // requests it knew about; answer any queued
                                // requests it had not yet resent so none are
                                // left hanging.
                                answer_unanswered_pending(stdout, &pending_lines).await;
                                ensure_daemon_ready_blocking(launcher).context(
                                    "Failed to recover daemon readiness before terminal adapter exit",
                                )?;
                                info!(
                                    "HTTP adapter verified daemon readiness before terminal exit"
                                );
                            }
                        }
                        return Ok(());
                    }
                    AdapterRetryDecision::Exhausted => match adapter_error {
                        AdapterError::Transport {
                            error, lost_line, ..
                        } => {
                            // The request whose send last failed is not in
                            // pending_lines; queue it so it is answered too.
                            if let Some(line) = lost_line {
                                pending_lines.push_back(line);
                            }
                            answer_unanswered_pending(stdout, &pending_lines).await;
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
                }
            }
        }
    }

    unreachable!("retry loop either returns success or exits with an error")
}

/// Answer every client request still queued in `pending_lines` with a
/// synthesized "connection lost" error before the adapter gives up.
///
/// In-flight requests (already on the wire) are answered by the forwarder when
/// the transport dies after output; this covers requests that were requeued for
/// replay but never made it back out because the retry budget was exhausted.
/// Without it, those requests would leave the MCP client hanging forever.
async fn answer_unanswered_pending<Out>(stdout: &mut Out, pending_lines: &VecDeque<Vec<u8>>)
where
    Out: AsyncWrite + Unpin,
{
    for line in pending_lines {
        if let Some(id) = request_id_from_line(line) {
            write_lost_connection_error(stdout, &id).await;
        }
    }
}

fn ensure_daemon_ready_blocking<C: DaemonAdapterControl>(launcher: &C) -> std::io::Result<()> {
    tokio::task::block_in_place(|| launcher.ensure_daemon_ready())
}
