use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::sidecar_protocol::{
    validate_batch_response, validate_query_response, validate_response_envelope,
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, RequestEnvelope,
    ResponseEnvelope, SIDECAR_EXPECTED_DIMS, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
};
use super::sidecar_supervisor::build_sidecar_launch_config;
use super::{DeviceInfo, EmbeddingProvider};

pub struct SidecarEmbeddingProvider {
    process: Mutex<SidecarProcess>,
    device: String,
    sidecar_runtime: String,
    model_id: String,
}

struct SidecarProcess {
    child: Child,
    stdin: ChildStdin,
    stdout_rx: Receiver<Result<String>>,
    request_seq: u64,
    response_timeout: Duration,
}

const DEFAULT_SIDECAR_TIMEOUT_MS: u64 = 5000;
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
        let mut command = Command::new(&launch.program);
        command.args(launch.args);
        for (key, value) in launch.env {
            command.env(key, value);
        }

        Self::spawn_from_command(command)
    }

    #[cfg(test)]
    pub(crate) fn try_new_for_command(program: String, args: Vec<String>) -> Result<Self> {
        let mut command = Command::new(program);
        command.args(args);
        Self::spawn_from_command(command)
    }

    fn spawn_from_command(mut command: Command) -> Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = command
            .spawn()
            .context("failed to spawn embedding sidecar")?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("sidecar stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("sidecar stdout unavailable"))?;
        let stdout_rx = spawn_stdout_reader(stdout);
        let response_timeout = read_response_timeout();

        let mut process = SidecarProcess {
            child,
            stdin,
            stdout_rx,
            request_seq: 0,
            response_timeout,
        };

        let health = match process.probe_readiness() {
            Ok(h) => h,
            Err(err) => {
                process.terminate();
                return Err(err);
            }
        };

        Ok(Self {
            process: Mutex::new(process),
            device: health.device.unwrap_or_else(|| "unknown".to_string()),
            sidecar_runtime: health.runtime.unwrap_or_else(|| "python-sidecar".to_string()),
            model_id: health.model_id.unwrap_or_else(|| "BAAI/bge-small-en-v1.5".to_string()),
        })
    }
}

impl EmbeddingProvider for SidecarEmbeddingProvider {
    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let mut process = self
            .process
            .lock()
            .map_err(|_| anyhow!("sidecar process lock poisoned"))?;

        let result: EmbedQueryResult = process.send_request(
            "embed_query",
            EmbedQueryRequest {
                text: text.to_string(),
            },
        )?;
        validate_query_response(&result)?;
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

        let result: EmbedBatchResult = process.send_request(
            "embed_batch",
            EmbedBatchRequest {
                texts: texts.to_vec(),
            },
        )?;
        validate_batch_response(&result, texts.len())?;
        Ok(result.vectors)
    }

    fn dimensions(&self) -> usize {
        SIDECAR_EXPECTED_DIMS
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: format!("python-sidecar ({})", self.sidecar_runtime),
            device: self.device.clone(),
            model_name: self.model_id.clone(),
            dimensions: SIDECAR_EXPECTED_DIMS,
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
        let request_id = self.next_request_id();
        let envelope = RequestEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: request_id.clone(),
            method: method.to_string(),
            params,
        };

        serde_json::to_writer(&mut self.stdin, &envelope)
            .with_context(|| format!("failed to encode sidecar request for method '{method}'"))?;
        self.stdin
            .write_all(b"\n")
            .with_context(|| format!("failed to write sidecar request for method '{method}'"))?;
        self.stdin
            .flush()
            .with_context(|| format!("failed to flush sidecar request for method '{method}'"))?;

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
                bail!("sidecar stream error while handling method '{method}': {err}{exit_info}");
            }
            Err(RecvTimeoutError::Timeout) => {
                bail!(
                    "timed out waiting for sidecar response for method '{method}' after {}ms",
                    timeout.as_millis()
                );
            }
            Err(RecvTimeoutError::Disconnected) => {
                bail!("sidecar stdout reader disconnected while handling method '{method}'");
            }
        };

        let envelope: ResponseEnvelope<Resp> = serde_json::from_str(line.trim())
            .with_context(|| format!("failed to decode sidecar response for method '{method}'"))?;
        validate_response_envelope(&envelope, &request_id)?;

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

        if let Some(dims) = health.dims {
            if dims != SIDECAR_EXPECTED_DIMS {
                bail!(
                    "sidecar health dimension mismatch: expected {}, got {}",
                    SIDECAR_EXPECTED_DIMS,
                    dims
                );
            }
        }

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
