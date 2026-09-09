use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::super::sidecar_protocol::{
    HealthResult, RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA,
    SIDECAR_PROTOCOL_VERSION, validate_health_response, validate_response_envelope,
};
use super::super::sidecar_supervisor::SidecarLaunchConfig;

/// Per-request timeout for sidecar embed_batch calls.
///
/// This must be generous enough for:
/// 1. The first batch's DirectML/CUDA graph compilation warm-up (~5-10s)
/// 2. Processing up to EMBEDDING_BATCH_SIZE texts through the model
/// 3. Larger models like CodeRankEmbed (768d, ~2x slower than BGE-small 384d)
///
/// Embedding is background work — the user isn't waiting on each batch.
/// Override with JULIE_EMBEDDING_SIDECAR_TIMEOUT_MS if needed.
pub(crate) const DEFAULT_SIDECAR_TIMEOUT_MS: u64 = 30_000;
pub(crate) const DEFAULT_SIDECAR_INIT_TIMEOUT_MS: u64 = 120_000;
pub(crate) const SHUTDOWN_TIMEOUT_MS: u64 = 500;

pub(crate) struct SidecarProcess {
    pub(crate) child: Child,
    stdin: ChildStdin,
    stdout_rx: Receiver<Result<String>>,
    request_seq: u64,
    #[allow(dead_code)]
    response_timeout: Duration,
    connection_fatal: bool,
}

impl SidecarProcess {
    pub(crate) fn mark_connection_fatal(&mut self) {
        self.connection_fatal = true;
        self.terminate();
    }

    pub(crate) fn take_connection_fatal(&mut self) -> bool {
        let was_fatal = self.connection_fatal;
        self.connection_fatal = false;
        was_fatal
    }

    pub(crate) fn next_request_id(&mut self) -> String {
        self.request_seq = self.request_seq.wrapping_add(1);
        format!("req-{}", self.request_seq)
    }

    #[allow(dead_code)]
    pub(crate) fn send_request<Params, Resp>(
        &mut self,
        method: &str,
        params: Params,
    ) -> Result<Resp>
    where
        Params: Serialize,
        Resp: DeserializeOwned,
    {
        self.send_request_with_timeout(method, params, self.response_timeout)
    }

    pub(crate) fn send_request_with_timeout<Params, Resp>(
        &mut self,
        method: &str,
        params: Params,
        timeout: Duration,
    ) -> Result<Resp>
    where
        Params: Serialize,
        Resp: DeserializeOwned,
    {
        self.connection_fatal = false;
        let request_id = self.next_request_id();
        let envelope = RequestEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: request_id.clone(),
            method: method.to_string(),
            params,
        };

        if let Err(err) = serde_json::to_writer(&mut self.stdin, &envelope) {
            self.mark_connection_fatal();
            return Err(err).with_context(|| {
                format!("failed to encode sidecar request for method '{method}'")
            });
        }
        if let Err(err) = self.stdin.write_all(b"\n") {
            self.mark_connection_fatal();
            return Err(err)
                .with_context(|| format!("failed to write sidecar request for method '{method}'"));
        }
        if let Err(err) = self.stdin.flush() {
            self.mark_connection_fatal();
            return Err(err)
                .with_context(|| format!("failed to flush sidecar request for method '{method}'"));
        }

        let line = match self.stdout_rx.recv_timeout(timeout) {
            Ok(Ok(line)) => line,
            Ok(Err(err)) => {
                // stdout closed = process likely crashed. Allow a short delay
                // for Windows to update the process handle state before checking
                // exit code (pipe close is detected before process termination).
                std::thread::sleep(Duration::from_millis(50));
                let exit_info = match self.child.try_wait() {
                    Ok(Some(status)) => format!(" (exit status: {status})"),
                    Ok(None) => " (process still running)".to_string(),
                    Err(e) => format!(" (could not check exit status: {e})"),
                };
                self.mark_connection_fatal();
                bail!("sidecar stream error while handling method '{method}': {err}{exit_info}");
            }
            Err(RecvTimeoutError::Timeout) => {
                self.mark_connection_fatal();
                bail!(
                    "timed out waiting for sidecar response for method '{method}' after {}ms",
                    timeout.as_millis()
                );
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.mark_connection_fatal();
                bail!("sidecar stdout reader disconnected while handling method '{method}'");
            }
        };

        let envelope: ResponseEnvelope<Resp> = match serde_json::from_str(line.trim()) {
            Ok(envelope) => envelope,
            Err(err) => {
                self.mark_connection_fatal();
                return Err(err).with_context(|| {
                    format!("failed to decode sidecar response for method '{method}'")
                });
            }
        };
        if let Err(err) = validate_response_envelope(&envelope, &request_id) {
            self.mark_connection_fatal();
            return Err(err);
        }

        // Application-level error — the Python protocol loop survived and sent a
        // well-formed error envelope, so the connection is healthy.  Do NOT mark
        // connection_fatal here; only transport/desync failures warrant a reset.
        if let Some(err) = envelope.error {
            bail!(
                "sidecar error for method '{method}': [{}] {}",
                err.code,
                err.message
            );
        }

        envelope
            .result
            .ok_or_else(|| anyhow!("sidecar response missing result for method '{method}'"))
    }

    pub(crate) fn probe_readiness(&mut self) -> Result<HealthResult> {
        let init_timeout = read_init_timeout();
        let health: HealthResult =
            self.send_request_with_timeout("health", serde_json::json!({}), init_timeout)?;
        validate_health_response(&health)?;
        if !health.ready {
            bail!("sidecar reported not ready in health probe");
        }

        Ok(health)
    }

    pub(crate) fn shutdown_and_terminate(&mut self) {
        let _ = self.send_request_with_timeout::<_, serde_json::Value>(
            "shutdown",
            serde_json::json!({}),
            Duration::from_millis(SHUTDOWN_TIMEOUT_MS),
        );
        self.terminate();
    }

    pub(crate) fn terminate(&mut self) {
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }

        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn spawn_process(
    launch_config: &SidecarLaunchConfig,
    response_timeout: Duration,
) -> Result<(SidecarProcess, HealthResult)> {
    let mut command = Command::new(&launch_config.program);
    command.args(&launch_config.args);
    for (key, value) in &launch_config.env {
        command.env(key, value);
    }

    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Null stderr so the parent's stderr handle is not inherited by the
        // sidecar child. On Windows, inherited handles can prevent the parent
        // from releasing file resources until the child exits, which causes
        // races when the new daemon starts before the old sidecar is done.
        .stderr(Stdio::null());

    // On Windows: CREATE_NO_WINDOW hides the console; CREATE_NEW_PROCESS_GROUP
    // assigns the child its own process group so Ctrl-C and SIGINT from the
    // parent are not delivered to the sidecar, and the child's handle
    // inheritance set is isolated from the parent.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }

    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to spawn embedding sidecar (program: {:?}, args: {:?})",
            launch_config.program, launch_config.args
        )
    })?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("sidecar stdin unavailable"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("sidecar stdout unavailable"))?;
    let stdout_rx = spawn_stdout_reader(stdout);

    let mut process = SidecarProcess {
        child,
        stdin,
        stdout_rx,
        request_seq: 0,
        response_timeout,
        connection_fatal: false,
    };

    let health = match process.probe_readiness() {
        Ok(h) => h,
        Err(err) => {
            process.terminate();
            return Err(err).with_context(|| {
                format!(
                    "sidecar process started but health check failed \
                     (program: {:?}). Check sidecar logs for import \
                     errors or missing dependencies.",
                    launch_config.program
                )
            });
        }
    };

    Ok((process, health))
}

fn spawn_stdout_reader(stdout: ChildStdout) -> Receiver<Result<String>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = tx.send(Err(anyhow!("sidecar stdout closed")));
                    break;
                }
                Ok(_) => {
                    if tx.send(Ok(line)).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(err.into()));
                    break;
                }
            }
        }
    });
    rx
}

pub(crate) fn read_response_timeout() -> Duration {
    let timeout_ms = std::env::var("JULIE_EMBEDDING_SIDECAR_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SIDECAR_TIMEOUT_MS);
    Duration::from_millis(timeout_ms)
}

pub(crate) fn read_init_timeout() -> Duration {
    let timeout_ms = std::env::var("JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SIDECAR_INIT_TIMEOUT_MS);
    Duration::from_millis(timeout_ms)
}
