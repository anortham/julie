//! Native shared broker embedding provider using `julie-semantic-sidecar`.

pub mod client;
pub mod decoders;
pub mod health;
pub mod launch;
pub mod lifecycle;

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Result, bail};
use serde::Serialize;
use tracing::debug;

use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity,
};

use crate::embeddings::factory::EmbeddingConfig;
use crate::embeddings::sidecar_protocol::{
    EmbedBatchRequest, EmbedQueryRequest, RequestEnvelope, SIDECAR_PROTOCOL_SCHEMA,
    SIDECAR_PROTOCOL_VERSION,
};

pub use client::{
    NativeClientConn, decode_native_batch_reply, decode_native_health_reply,
    decode_native_query_reply, is_connection_dropped, read_line_bounded,
};
pub use health::{query_and_validate_health, validate_native_health};
pub use launch::{
    BrokerPaths, DEFAULT_NATIVE_MODEL, NativeLaunchConfig, derive_broker_paths,
    find_and_hash_sidecar_binary, run_prepare, spawn_broker, verify_launched_child_sha,
};

#[derive(Clone)]
struct NativeRuntimeFacts {
    device_info: DeviceInfo,
    accelerated: Option<bool>,
    degraded_reason: Option<String>,
}

/// High-performance native embedding provider communicating with `julie-semantic-sidecar`.
pub struct NativeEmbeddingProvider {
    config: NativeLaunchConfig,
    conn: Mutex<Option<NativeClientConn>>,
    _child_stdin: Mutex<Option<std::process::ChildStdin>>,
    identity: EncoderIdentity,
    runtime_facts: Mutex<NativeRuntimeFacts>,
    request_counter: AtomicU64,
    running_executable_sha: Mutex<Option<String>>,
}

impl NativeEmbeddingProvider {
    /// Attempts to acquire or launch a native sidecar broker and connects to it.
    pub fn try_new(embedding_config: &EmbeddingConfig) -> Result<Self> {
        let launch_config = NativeLaunchConfig::try_new(
            embedding_config.native_program.as_deref(),
            embedding_config.native_model.as_deref(),
            embedding_config.cache_dir.as_deref(),
        )?;

        let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(10));
        let (conn, child_stdin, identity, device_info, health, current_sha) =
            Self::launch_and_attach(&launch_config, &budget, None)?;

        let runtime_facts = NativeRuntimeFacts {
            device_info,
            accelerated: health.accelerated,
            degraded_reason: health.degraded_reason,
        };

        Ok(Self {
            config: launch_config,
            conn: Mutex::new(Some(conn)),
            _child_stdin: Mutex::new(child_stdin),
            identity,
            runtime_facts: Mutex::new(runtime_facts),
            request_counter: AtomicU64::new(1),
            running_executable_sha: Mutex::new(Some(current_sha)),
        })
    }

    /// Launches or connects to a native sidecar broker, coordinating across processes.
    pub fn launch_and_attach(
        config: &NativeLaunchConfig,
        budget: &EmbeddingRequestBudget,
        expected_identity: Option<&EncoderIdentity>,
    ) -> Result<(
        NativeClientConn,
        Option<std::process::ChildStdin>,
        EncoderIdentity,
        DeviceInfo,
        crate::embeddings::sidecar_protocol::HealthResult,
        String,
    )> {
        lifecycle::launch_and_attach(config, budget, expected_identity)
    }

    /// Constructs a provider from an existing, connected client and identity (for tests).
    pub fn from_connected(
        config: NativeLaunchConfig,
        client: NativeClientConn,
        identity: EncoderIdentity,
        device_info: DeviceInfo,
    ) -> Self {
        let runtime_facts = NativeRuntimeFacts {
            device_info,
            accelerated: None,
            degraded_reason: None,
        };
        let sha = config.executable_sha256.clone();
        Self {
            config,
            conn: Mutex::new(Some(client)),
            _child_stdin: Mutex::new(None),
            identity,
            runtime_facts: Mutex::new(runtime_facts),
            request_counter: AtomicU64::new(1),
            running_executable_sha: Mutex::new(Some(sha)),
        }
    }

    fn ensure_connected(
        &self,
        guard: &mut Option<NativeClientConn>,
        budget: &EmbeddingRequestBudget,
    ) -> Result<()> {
        budget.check_budget()?;
        if guard.is_some() {
            return Ok(());
        }

        let remaining = budget.remaining_time();
        if remaining.is_zero() {
            bail!("embedding request deadline exceeded before connecting to native broker");
        }

        let (conn, child_stdin, _, replacement_dev, replacement_health, replacement_sha) =
            Self::launch_and_attach(&self.config, budget, Some(&self.identity))?;

        if let Ok(mut stdin_guard) = self._child_stdin.lock() {
            *stdin_guard = child_stdin;
        }
        if let Ok(mut facts_guard) = self.runtime_facts.lock() {
            *facts_guard = NativeRuntimeFacts {
                device_info: replacement_dev,
                accelerated: replacement_health.accelerated,
                degraded_reason: replacement_health.degraded_reason,
            };
        }
        if let Ok(mut sha_guard) = self.running_executable_sha.lock() {
            *sha_guard = Some(replacement_sha);
        }

        budget.check_budget()?;
        *guard = Some(conn);
        Ok(())
    }

    fn execute_round_trip<P: Serialize, R>(
        &self,
        method: &str,
        params: P,
        budget: &EmbeddingRequestBudget,
        decode: impl Fn(&[u8], &str) -> Result<R>,
    ) -> Result<R> {
        budget.check_budget()?;
        let mut attempts = 0;

        loop {
            attempts += 1;
            budget.check_budget()?;

            // Non-blocking mutex admission bounded by request budget
            let mut guard = loop {
                budget.check_budget()?;
                match self.conn.try_lock() {
                    Ok(g) => break g,
                    Err(std::sync::TryLockError::WouldBlock) => {
                        let rem = budget.remaining_time();
                        if rem.is_zero() {
                            bail!(
                                "embedding request deadline exceeded while waiting for provider lock"
                            );
                        }
                        std::thread::sleep(Duration::from_millis(5).min(rem));
                    }
                    Err(std::sync::TryLockError::Poisoned(_)) => {
                        bail!("native provider mutex poisoned");
                    }
                }
            };

            // Check budget before connecting
            budget.check_budget()?;
            if budget.remaining_time().is_zero() {
                bail!("embedding request deadline exceeded");
            }

            self.ensure_connected(&mut guard, budget)?;

            // Recompute remaining budget AFTER connection/reconnect completes
            budget.check_budget()?;
            let remaining = budget.remaining_time();
            if remaining.is_zero() {
                bail!("embedding request deadline exceeded after connecting to native broker");
            }

            let conn = guard.as_mut().expect("connected");
            let req_num = self.request_counter.fetch_add(1, Ordering::Relaxed);
            let req_id = format!("req-{req_num}");

            let envelope = RequestEnvelope {
                schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
                version: SIDECAR_PROTOCOL_VERSION,
                request_id: req_id.clone(),
                method: method.to_string(),
                params: serde_json::to_value(&params)?,
            };

            let req_bytes = serde_json::to_vec(&envelope)?;

            match conn.round_trip(&req_bytes, Some(remaining)) {
                Ok(resp_bytes) => {
                    budget.check_budget()?;
                    return decode(&resp_bytes, &req_id);
                }
                Err(err) if attempts < 2 && is_connection_dropped(&err) => {
                    // Drop dead connection, discarding any stale stream state
                    *guard = None;
                    if budget.is_expired() || budget.is_cancelled() {
                        return Err(err.into());
                    }
                    debug!("native broker connection dropped; retrying once within budget");
                    continue;
                }
                Err(err) => {
                    // On timeout or protocol failure, always drop connection to avoid desync
                    *guard = None;
                    return Err(err.into());
                }
            }
        }
    }
}

impl EmbeddingProvider for NativeEmbeddingProvider {
    fn embed_query(&self, text: &str, budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        budget.check_budget()?;
        let params = EmbedQueryRequest {
            text: text.to_string(),
            remaining_budget_ms: Some(budget.remaining_time().as_millis() as u64),
        };

        self.execute_round_trip("embed_query", params, budget, |bytes, id| {
            decode_native_query_reply(bytes, id, self.identity.dimensions)
        })
    }

    fn embed_batch(
        &self,
        texts: &[String],
        budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        budget.check_budget()?;
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let params = EmbedBatchRequest {
            texts: texts.to_vec(),
            remaining_budget_ms: Some(budget.remaining_time().as_millis() as u64),
        };

        let expected_count = texts.len();
        self.execute_round_trip("embed_batch", params, budget, |bytes, id| {
            decode_native_batch_reply(bytes, id, self.identity.dimensions, expected_count)
        })
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(self.identity.clone())
    }

    fn dimensions(&self) -> usize {
        self.identity.dimensions
    }

    fn device_info(&self) -> DeviceInfo {
        self.runtime_facts
            .lock()
            .map(|f| f.device_info.clone())
            .unwrap_or_else(|_| DeviceInfo {
                runtime: "native".to_string(),
                device: "unknown".to_string(),
                model_name: self.identity.model_id.clone(),
                dimensions: self.identity.dimensions,
            })
    }

    fn accelerated(&self) -> Option<bool> {
        self.runtime_facts.lock().ok().and_then(|f| f.accelerated)
    }

    fn degraded_reason(&self) -> Option<String> {
        self.runtime_facts
            .lock()
            .ok()
            .and_then(|f| f.degraded_reason.clone())
    }

    fn running_executable_sha(&self) -> Option<String> {
        self.running_executable_sha
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
    }

    fn health_check(&self, budget: &EmbeddingRequestBudget) -> Result<()> {
        budget.check_budget()?;
        self.execute_round_trip("health", serde_json::json!({}), budget, |bytes, id| {
            let health = decode_native_health_reply(bytes, id)?;
            let (identity, _) = validate_native_health(&health, Some(&self.config.model_id))?;
            if identity != self.identity {
                bail!(
                    "reconnected broker identity mismatch (expected {}, got {})",
                    self.identity.storage_key().unwrap_or_default(),
                    identity.storage_key().unwrap_or_default()
                );
            }
            Ok(())
        })
    }

    fn shutdown(&self) {
        if let Ok(mut guard) = self.conn.lock() {
            *guard = None;
        }
    }
}
