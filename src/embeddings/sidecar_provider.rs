use std::io::{BufRead, BufReader, Write};
#[cfg(test)]
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::sidecar_protocol::{
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult,
    RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
    validate_batch_response, validate_health_response, validate_query_response,
    validate_response_envelope,
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
    accelerated: bool,
    degraded_reason: Option<String>,
    /// Count of consecutive fatal failures across all respawn attempts.
    /// Resets to 0 on the first successful request. Once it reaches
    /// FATAL_THRESHOLD the provider is permanently disabled and stops
    /// attempting to respawn.
    consecutive_fatal_failures: AtomicU32,
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
            accelerated: health.accelerated.unwrap_or(false),
            degraded_reason: health.degraded_reason,
            consecutive_fatal_failures: AtomicU32::new(0),
        })
    }

    fn reset_process_if_fatal(&self, process: &mut SidecarProcess) -> Result<()> {
        if !process.take_connection_fatal() {
            return Ok(());
        }

        const FATAL_THRESHOLD: u32 = 3;
        let failures = self
            .consecutive_fatal_failures
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        if failures >= FATAL_THRESHOLD {
            bail!(
                "embedding sidecar permanently disabled after {} consecutive fatal failures",
                failures
            );
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
        // Successful request: reset the consecutive failure counter.
        self.consecutive_fatal_failures.store(0, Ordering::Relaxed);
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
        // Successful request: reset the consecutive failure counter.
        self.consecutive_fatal_failures.store(0, Ordering::Relaxed);
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

    fn accelerated(&self) -> Option<bool> {
        Some(self.accelerated)
    }

    fn degraded_reason(&self) -> Option<String> {
        self.degraded_reason.clone()
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
        // Drop must not block on I/O (no graceful shutdown write).
        // Graceful shutdown is handled by the explicit shutdown() method.
        match self.process.lock() {
            Ok(mut process) => process.terminate(),
            Err(poisoned) => poisoned.into_inner().terminate(),
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
        validate_health_response(&health)?;
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

#[cfg(test)]
#[cfg(feature = "embeddings-sidecar")]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::SidecarEmbeddingProvider;
    use crate::embeddings::EmbeddingProvider;

    fn test_python_interpreter() -> String {
        if let Ok(v) = std::env::var("JULIE_TEST_PYTHON") {
            let v = v.trim().to_string();
            if !v.is_empty() {
                return v;
            }
        }
        let candidates = if cfg!(target_os = "windows") {
            vec!["python", "py", "python3"]
        } else {
            vec!["python3", "python"]
        };
        for candidate in candidates {
            let ok = Command::new(candidate)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if ok {
                return candidate.to_string();
            }
        }
        panic!("No Python interpreter found; set JULIE_TEST_PYTHON");
    }

    /// Verify the circuit breaker: after FATAL_THRESHOLD consecutive fatal
    /// failures the provider reports "permanently disabled" instead of
    /// attempting yet another respawn.
    ///
    /// The fake sidecar passes the health check (so construction succeeds)
    /// but immediately exits on any embed request, triggering a fatal error
    /// each time. Without the circuit breaker the provider would respawn
    /// indefinitely; with it the 3rd failure permanently disables the
    /// provider and subsequent calls fail fast without spawning new processes.
    #[test]
    fn test_circuit_breaker_permanently_disables_after_consecutive_fatal_failures() {
        // Python sidecar: passes health, crashes immediately on embed.
        let script = r#"
import json, sys
while True:
    line = sys.stdin.readline()
    if not line:
        break
    req = json.loads(line)
    method = req.get("method", "")
    req_id = req.get("request_id", "")
    if method == "health":
        resp = {"schema": "julie.embedding.sidecar", "version": 1,
                "request_id": req_id,
                "result": {"ready": True, "runtime": "crash-sidecar",
                           "device": "cpu", "dims": 4}}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
    elif method.startswith("embed"):
        # Crash without sending a response -- marks connection as fatal.
        sys.exit(1)
    elif method == "shutdown":
        resp = {"schema": "julie.embedding.sidecar", "version": 1,
                "request_id": req_id, "result": {"stopping": True}}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
        break
"#;

        let provider = SidecarEmbeddingProvider::try_new_for_command_with_timeout(
            test_python_interpreter(),
            vec!["-u".to_string(), "-c".to_string(), script.to_string()],
            Duration::from_secs(5),
        )
        .expect("provider construction should succeed (health check passes)");

        // Calls 1 and 2: fatal errors, each triggers a successful respawn.
        // The "permanently disabled" message should NOT appear yet.
        for i in 1..=2 {
            let err = provider
                .embed_query("x")
                .expect_err("embed should fail due to crash sidecar");
            assert!(
                !err.to_string().contains("permanently disabled"),
                "failure #{i} should not yet trigger circuit breaker, got: {err}"
            );
        }

        // Call 3: fatal error + circuit breaker fires -- permanently disabled.
        let err3 = provider
            .embed_query("x")
            .expect_err("3rd embed should fail with circuit breaker");
        assert!(
            err3.to_string().contains("permanently disabled"),
            "3rd consecutive failure should trigger circuit breaker, got: {err3}"
        );

        // Call 4: provider is now permanently disabled, should fail fast.
        let err4 = provider
            .embed_query("x")
            .expect_err("4th embed should fail immediately (permanently disabled)");
        assert!(
            err4.to_string().contains("permanently disabled"),
            "4th call should still be permanently disabled, got: {err4}"
        );
    }
}
