//! One `julie-semantic-sidecar serve` child, NDJSON over its stdin and stdout.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use julie_core::embeddings_contract::EmbeddingRequestBudget;

use crate::embeddings::sidecar_protocol::{
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult,
    RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
    validate_batch_response, validate_health_response, validate_query_response,
    validate_response_envelope,
};

/// Maximum payload size in bytes (32 MiB).
pub const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;

/// Running sidecar child process communicating via NDJSON over stdio.
pub struct SidecarChild {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    next_id: u64,
}

impl SidecarChild {
    /// Spawns a new sidecar child process with `--model` in serve mode.
    pub fn spawn(executable: &Path, model_id: &str) -> Result<Self> {
        let mut child = Command::new(executable)
            .arg("serve")
            .arg("--model")
            .arg(model_id)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawn {}", executable.display()))?;
        let stdin = child.stdin.take().context("child stdin")?;
        let stdout = child.stdout.take().context("child stdout")?;
        Ok(Self {
            child,
            stdin,
            lines: reader_thread(stdout),
            next_id: 1,
        })
    }

    /// Returns the OS process ID of the child process.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Checks if the child process is currently alive.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Sends a health check request to the child.
    pub fn health(&mut self, budget: &EmbeddingRequestBudget) -> Result<HealthResult> {
        let health: HealthResult = self.round_trip("health", serde_json::json!({}), budget)?;
        validate_health_response(&health)?;
        Ok(health)
    }

    /// Generates an embedding vector for a single query string.
    pub fn embed_query(&mut self, text: &str, budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        let result: EmbedQueryResult = self.round_trip(
            "embed_query",
            EmbedQueryRequest {
                text: text.to_string(),
                remaining_budget_ms: Some(budget.remaining_time().as_millis() as u64),
            },
            budget,
        )?;
        validate_query_response(&result, result.dims)?;
        Ok(result.vector)
    }

    /// Generates embedding vectors for a batch of strings.
    pub fn embed_batch(
        &mut self,
        texts: &[String],
        budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        let result: EmbedBatchResult = self.round_trip(
            "embed_batch",
            EmbedBatchRequest {
                texts: texts.to_vec(),
                remaining_budget_ms: Some(budget.remaining_time().as_millis() as u64),
            },
            budget,
        )?;
        validate_batch_response(&result, texts.len(), result.dims)?;
        Ok(result.vectors)
    }

    /// Gracefully shuts down the child process, falling back to SIGKILL.
    pub fn shutdown(mut self) {
        let _ = self.round_trip::<_, serde_json::Value>(
            "shutdown",
            serde_json::json!({}),
            &EmbeddingRequestBudget::with_timeout(Duration::from_millis(500)),
        );
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn round_trip<P: Serialize, R: DeserializeOwned>(
        &mut self,
        method: &str,
        params: P,
        budget: &EmbeddingRequestBudget,
    ) -> Result<R> {
        budget.check_budget()?;
        let id = self.next_id.to_string();
        self.next_id += 1;
        let request = RequestEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: id.clone(),
            method: method.to_string(),
            params,
        };
        let mut line = serde_json::to_vec(&request)?;
        line.push(b'\n');
        self.stdin.write_all(&line).context("write to sidecar")?;
        self.stdin.flush()?;
        let raw = match self.lines.recv_timeout(budget.remaining_time()) {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(err)) => bail!("sidecar stdout: {err}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                bail!("sidecar {method} exceeded the request deadline")
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("sidecar exited"),
        };
        let envelope: ResponseEnvelope<R> =
            serde_json::from_slice(&raw).context("sidecar reply")?;
        validate_response_envelope(&envelope, &id)?;
        match (envelope.result, envelope.error) {
            (Some(result), _) => Ok(result),
            (None, Some(err)) => bail!("sidecar {method}: {} ({})", err.message, err.code),
            (None, None) => bail!("sidecar {method}: empty reply"),
        }
    }
}

impl Drop for SidecarChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn reader_thread(stdout: ChildStdout) -> mpsc::Receiver<std::io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut buf = Vec::new();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(n) if n > MAX_PAYLOAD_BYTES => {
                    let _ = tx.send(Err(std::io::Error::other("sidecar line over 32 MiB")));
                    break;
                }
                Ok(_) => {
                    if tx.send(Ok(buf)).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(err));
                    break;
                }
            }
        }
    });
    rx
}
