use std::io::{BufRead, BufReader, Write};
#[cfg(test)]
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::sidecar_protocol::{
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, RequestEnvelope,
    ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION, validate_batch_response,
    validate_query_response, validate_response_envelope,
};
use super::sidecar_supervisor::{SidecarLaunchConfig, build_sidecar_launch_config};
use super::{DeviceInfo, EmbeddingProvider};

pub struct SidecarEmbeddingProvider {
    process: Mutex<SidecarProcess>,
    launch_config: SidecarLaunchConfig,
    response_timeout: Duration,
    device: String,
    sidecar_runtime: String,
    model_id: String,
    expected_dims: usize,
}

struct SidecarProcess {
    child: Child,
    stdin: ChildStdin,
    stdout_rx: Receiver<Result<String>>,
    request_seq: u64,
    response_timeout: Duration,
    connection_fatal: bool,
}

/// Per-request timeout for sidecar embed_batch calls.
///
/// This must be generous enough for:
/// 1. The first batch's DirectML/CUDA graph compilation warm-up (~5-10s)
/// 2. Processing up to EMBEDDING_BATCH_SIZE texts through the model
/// 3. Larger models like CodeRankEmbed (768d, ~2x slower than BGE-small 384d)
///
/// Embedding is background work — the user isn't waiting on each batch.
/// Override with JULIE_EMBEDDING_SIDECAR_TIMEOUT_MS if needed.
const DEFAULT_SIDECAR_TIMEOUT_MS: u64 = 30_000;
const DEFAULT_SIDECAR_INIT_TIMEOUT_MS: u64 = 120_000;
const SHUTDOWN_TIMEOUT_MS: u64 = 500;

#[derive(Debug, Deserialize)]
struct HealthResult {
    ready: bool,
    #[serde(default)]
    dims: Option<usize>,
    #[serde(default)]
    device: Option<String>,
    #[serde(default)]
    runtime: Option<String>,
    #[serde(default)]
    model_id: Option<String>,
}

impl SidecarEmbeddingProvider {
    pub fn try_new() -> Result<Self> {
        let launch = build_sidecar_launch_config()?;
        Self::spawn_from_launch_config(launch, read_response_timeout())
    }

    #[cfg(test)]
    pub(crate) fn try_new_for_command(program: String, args: Vec<String>) -> Result<Self> {
        Self::try_new_for_command_with_timeout(program, args, read_response_timeout())
    }

    #[cfg(test)]
    pub(crate) fn try_new_for_command_with_timeout(
        program: String,
        args: Vec<String>,
        response_timeout: Duration,
    ) -> Result<Self> {
        let launch = SidecarLaunchConfig {
            program: PathBuf::from(program),
            args,
            env: Vec::new(),
        };
        Self::spawn_from_launch_config(launch, response_timeout)
    }

    fn spawn_from_launch_config(
        launch_config: SidecarLaunchConfig,
        response_timeout: Duration,
    ) -> Result<Self> {
        let (process, health) = spawn_process(&launch_config, response_timeout)?;

        let expected_dims = health.dims.unwrap_or(384);

        Ok(Self {
            process: Mutex::new(process),
            launch_config,
            response_timeout,
            device: health.device.unwrap_or_else(|| "unknown".to_string()),
            sidecar_runtime: health
                .runtime
                .unwrap_or_else(|| "python-sidecar".to_string()),
            model_id: health
                .model_id
                .unwrap_or_else(|| "BAAI/bge-small-en-v1.5".to_string()),
            expected_dims,
        })
    }

    fn reset_process_if_fatal(&self, process: &mut SidecarProcess) -> Result<()> {
        if !process.take_connection_fatal() {
            return Ok(());
        }

        process.terminate();
        let (replacement, _) = spawn_process(&self.launch_config, self.response_timeout)
            .context("failed to respawn sidecar process after connection-fatal error")?;
        *process = replacement;
        Ok(())
    }
}

fn spawn_process(
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
        .stderr(Stdio::inherit());

    // On Windows, prevent the sidecar from opening a visible console window.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
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

impl EmbeddingProvider for SidecarEmbeddingProvider {
    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let mut process = self
            .process
            .lock()
            .map_err(|_| anyhow!("sidecar process lock poisoned"))?;

        let result: EmbedQueryResult = match process.send_request(
            "embed_query",
            EmbedQueryRequest {
                text: text.to_string(),
            },
        ) {
            Ok(result) => result,
            Err(err) => {
                self.reset_process_if_fatal(&mut process)?;
                return Err(err);
            }
        };
        validate_query_response(&result, self.expected_dims)?;
        Ok(result.vector)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut process = self
            .process
            .lock()
            .map_err(|_| anyhow!("sidecar process lock poisoned"))?;

        let result: EmbedBatchResult = match process.send_request(
            "embed_batch",
            EmbedBatchRequest {
                texts: texts.to_vec(),
            },
        ) {
            Ok(result) => result,
            Err(err) => {
                self.reset_process_if_fatal(&mut process)?;
                return Err(err);
            }
        };
        validate_batch_response(&result, texts.len(), self.expected_dims)?;
        Ok(result.vectors)
    }

    fn dimensions(&self) -> usize {
        self.expected_dims
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: format!("python-sidecar ({})", self.sidecar_runtime),
            device: self.device.clone(),
            model_name: self.model_id.clone(),
            dimensions: self.expected_dims,
        }
    }

    fn shutdown(&self) {
        match self.process.lock() {
            Ok(mut process) => process.shutdown_and_terminate(),
            Err(poisoned) => {
                let mut process = poisoned.into_inner();
                process.terminate();
            }
        }
    }
}

impl Drop for SidecarEmbeddingProvider {
    fn drop(&mut self) {
        match self.process.lock() {
            Ok(mut process) => process.shutdown_and_terminate(),
            Err(poisoned) => {
                let mut process = poisoned.into_inner();
                process.terminate();
            }
        }
    }
}

impl SidecarProcess {
    fn mark_connection_fatal(&mut self) {
        self.connection_fatal = true;
        self.terminate();
    }

    fn take_connection_fatal(&mut self) -> bool {
        let was_fatal = self.connection_fatal;
        self.connection_fatal = false;
        was_fatal
    }

    fn next_request_id(&mut self) -> String {
        self.request_seq = self.request_seq.wrapping_add(1);
        format!("req-{}", self.request_seq)
    }

    fn send_request<Params, Resp>(&mut self, method: &str, params: Params) -> Result<Resp>
    where
        Params: Serialize,
        Resp: DeserializeOwned,
    {
        self.send_request_with_timeout(method, params, self.response_timeout)
    }

    fn send_request_with_timeout<Params, Resp>(
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

    fn probe_readiness(&mut self) -> Result<HealthResult> {
        let init_timeout = read_init_timeout();
        let health: HealthResult =
            self.send_request_with_timeout("health", serde_json::json!({}), init_timeout)?;
        if !health.ready {
            bail!("sidecar reported not ready in health probe");
        }

        // Dimensions are validated post-construction by comparing provider.dimensions()
        // with the stored embedding config. The sidecar reports its dims here; we just
        // store them rather than validating against a hardcoded constant.

        Ok(health)
    }

    fn shutdown_and_terminate(&mut self) {
        let _ = self.send_request_with_timeout::<_, serde_json::Value>(
            "shutdown",
            serde_json::json!({}),
            Duration::from_millis(SHUTDOWN_TIMEOUT_MS),
        );
        self.terminate();
    }

    fn terminate(&mut self) {
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }

        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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

fn read_response_timeout() -> Duration {
    let timeout_ms = std::env::var("JULIE_EMBEDDING_SIDECAR_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SIDECAR_TIMEOUT_MS);
    Duration::from_millis(timeout_ms)
}

fn read_init_timeout() -> Duration {
    let timeout_ms = std::env::var("JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_SIDECAR_INIT_TIMEOUT_MS);
    Duration::from_millis(timeout_ms)
}
