//! Thin RPC-client `EmbeddingProvider` for the resident embedding-host (Phase 3b).
//!
//! Implements [`EmbeddingProvider`] over [`HostClientConn`]: lazy-connects on
//! first use, runs a health handshake to populate the cached dimensions and
//! device info, then forwards `embed_query` / `embed_batch` calls over the
//! blocking newline-delimited transport. On a broken-pipe I/O error the cached
//! connection is dropped and the call is retried exactly once (one reconnect
//! + re-handshake).

use std::io;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity,
};

use super::host_transport::{
    DEFAULT_RPC_TIMEOUT, HostAddress, HostClientConn, resolve_rpc_timeout,
};
use super::sidecar_protocol::{
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult,
    RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
    check_reconnect_health_match, validate_batch_response, validate_health_response,
    validate_query_response, validate_response_envelope,
};

use super::rpc_client_types::{CachedHealth, ConnInner};

// ---------------------------------------------------------------------------
// Public provider
// ---------------------------------------------------------------------------

/// Thin RPC client that implements [`EmbeddingProvider`] by forwarding over
/// the blocking [`HostClientConn`] transport to the resident embedding-host.
///
/// Interior mutability via `Mutex<Option<ConnInner>>` satisfies the `&self`
/// requirement of the trait while allowing lazy connect and reconnect-once.
pub struct RpcEmbeddingProvider {
    addr: HostAddress,
    /// `None` while disconnected; `Some` while a live connection is held.
    conn: Mutex<Option<ConnInner>>,
    /// Populated once from the health handshake at first connect.
    cached: OnceLock<CachedHealth>,
}

impl RpcEmbeddingProvider {
    /// Create a new provider. Does **not** connect until first use.
    pub fn new(addr: HostAddress) -> Self {
        Self {
            addr,
            conn: Mutex::new(None),
            cached: OnceLock::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Connection management
    // -----------------------------------------------------------------------

    /// Ensure the guard holds a live connection. If `guard` is `None`:
    /// connects, runs a health handshake, populates the cache (once).
    ///
    /// Returns `io::Error` so callers can distinguish transport errors from
    /// deserialization / protocol errors.
    fn ensure_connected(
        &self,
        guard: &mut Option<ConnInner>,
        timeout: Option<Duration>,
        deadline: Option<Instant>,
    ) -> io::Result<()> {
        if guard.is_some() {
            return Ok(());
        }
        let new_conn = HostClientConn::connect_with_timeout(&self.addr, timeout)?;
        let mut inner = ConnInner {
            conn: new_conn,
            request_seq: 0,
        };
        let health = Self::do_health_handshake(&mut inner, deadline, timeout)?;
        if let Some(existing) = self.cached.get() {
            if let Err(mismatch) = check_reconnect_health_match(&existing.health, &health) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("reconnect health mismatch: {mismatch}"),
                ));
            }
        }
        self.cached
            .get_or_init(|| CachedHealth::from_health(health));
        *guard = Some(inner);
        Ok(())
    }

    /// Send a `health` request on a freshly opened connection and return the
    /// validated [`HealthResult`].
    fn do_health_handshake(
        inner: &mut ConnInner,
        deadline: Option<Instant>,
        per_read_timeout: Option<Duration>,
    ) -> io::Result<HealthResult> {
        let request_id = inner.next_request_id();
        let envelope = RequestEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: request_id.clone(),
            method: "health".to_string(),
            params: serde_json::json!({}),
        };
        let line = serde_json::to_string(&envelope)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let resp_line = inner
            .conn
            .round_trip_with_deadline(&line, deadline, per_read_timeout)?;
        let resp: ResponseEnvelope<HealthResult> = serde_json::from_str(resp_line.trim())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        validate_response_envelope(&resp, &request_id)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        if let Some(err) = &resp.error {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("health error from host: [{}] {}", err.code, err.message),
            ));
        }
        let health = resp
            .result
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing health result"))?;
        validate_health_response(&health)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        if !health.ready {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "host not ready"));
        }
        Ok(health)
    }

    // -----------------------------------------------------------------------
    // Request dispatch
    // -----------------------------------------------------------------------

    /// Perform one request/response round-trip on the connection held in
    /// `guard` (which must be `Some` on entry). Returns `(response_line,
    /// request_id)`.
    fn attempt_once(
        guard: &mut Option<ConnInner>,
        method: &str,
        params: &serde_json::Value,
        deadline: Option<Instant>,
        per_read_timeout: Option<Duration>,
    ) -> io::Result<(String, String)> {
        let inner = guard.as_mut().expect("ConnInner must be Some");
        let request_id = inner.next_request_id();
        let envelope = RequestEnvelope {
            schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
            version: SIDECAR_PROTOCOL_VERSION,
            request_id: request_id.clone(),
            method: method.to_string(),
            params: params.clone(),
        };
        let line = serde_json::to_string(&envelope)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let resp_line = inner
            .conn
            .round_trip_with_deadline(&line, deadline, per_read_timeout)?;
        Ok((resp_line, request_id))
    }

    /// Serialize `params`, send to `method`, and deserialize the response.
    ///
    /// On a broken-pipe / EOF I/O error the cached connection is dropped and
    /// the call is retried exactly once (reconnect + re-handshake + retry).
    fn send_request<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: P,
        budget: &EmbeddingRequestBudget,
    ) -> Result<R> {
        let params_val = serde_json::to_value(params)
            .map_err(|e| anyhow!("failed to serialize {method} params: {e}"))?;

        // Non-blocking mutex admission with budget polling
        let mut guard = loop {
            budget.check_budget()?;
            match self.conn.try_lock() {
                Ok(g) => break g,
                Err(std::sync::TryLockError::WouldBlock) => {
                    let rem = budget.remaining_time();
                    if rem.is_zero() {
                        bail!(
                            "embedding request deadline exceeded while waiting for rpc provider lock"
                        );
                    }
                    std::thread::sleep(Duration::from_millis(5).min(rem));
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    bail!("RpcEmbeddingProvider: mutex poisoned")
                }
            }
        };

        budget.check_budget()?;
        let remaining = budget.remaining_time();
        if remaining.is_zero() {
            bail!("embedding request deadline exceeded");
        }
        let default_rpc_timeout = resolve_rpc_timeout().unwrap_or(DEFAULT_RPC_TIMEOUT);
        let clamped_timeout = default_rpc_timeout.min(remaining);

        self.ensure_connected(&mut guard, Some(clamped_timeout), Some(budget.deadline))
            .map_err(|e| anyhow!("embedding host connect: {e}"))?;

        let remaining = budget.remaining_time();
        if remaining.is_zero() {
            bail!("embedding request deadline exceeded after connect");
        }
        if let Some(inner) = guard.as_mut() {
            let _ = inner
                .conn
                .set_timeout(Some(default_rpc_timeout.min(remaining)));
        }

        let (resp_line, request_id) = match Self::attempt_once(
            &mut guard,
            method,
            &params_val,
            Some(budget.deadline),
            Some(clamped_timeout),
        ) {
            Ok(pair) => pair,
            Err(e) if is_connection_dropped(&e) => {
                // Drop the dead connection and reconnect exactly once.
                *guard = None;
                budget.check_budget()?;
                let rem = budget.remaining_time();
                if rem.is_zero() {
                    bail!("embedding request deadline exceeded before reconnect");
                }
                let reconnect_timeout = default_rpc_timeout.min(rem);
                self.ensure_connected(&mut guard, Some(reconnect_timeout), Some(budget.deadline))
                    .map_err(|e| anyhow!("embedding host reconnect: {e}"))?;
                let rem2 = budget.remaining_time();
                if rem2.is_zero() {
                    bail!("embedding request deadline exceeded after reconnect");
                }
                if let Some(inner) = guard.as_mut() {
                    let _ = inner.conn.set_timeout(Some(default_rpc_timeout.min(rem2)));
                }
                Self::attempt_once(
                    &mut guard,
                    method,
                    &params_val,
                    Some(budget.deadline),
                    Some(default_rpc_timeout.min(rem2)),
                )
                .map_err(|e| anyhow!("{method} failed after reconnect: {e}"))?
            }
            Err(e) => return Err(anyhow!("{method} io error: {e}")),
        };

        let res = Self::parse_response::<R>(&resp_line, method, &request_id)?;
        budget.check_budget()?;
        Ok(res)
    }

    /// Deserialize and validate one raw response line.
    fn parse_response<R: DeserializeOwned>(
        line: &str,
        method: &str,
        request_id: &str,
    ) -> Result<R> {
        let env: ResponseEnvelope<R> = serde_json::from_str(line.trim())
            .map_err(|e| anyhow!("failed to decode {method} response: {e}"))?;
        validate_response_envelope(&env, request_id)?;
        if let Some(err) = env.error {
            bail!("{method} host error: [{}] {}", err.code, err.message);
        }
        env.result
            .ok_or_else(|| anyhow!("{method} response missing result"))
    }

    // -----------------------------------------------------------------------
    // Cache access
    // -----------------------------------------------------------------------

    /// Return the cached health info, connecting lazily if not yet populated.
    fn get_cached(&self) -> Result<&CachedHealth> {
        if let Some(c) = self.cached.get() {
            return Ok(c);
        }
        // Not yet connected — trigger connection + health handshake.
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| anyhow!("RpcEmbeddingProvider: mutex poisoned"))?;
        let timeout = resolve_rpc_timeout();
        let deadline = timeout.map(|t| Instant::now() + t);
        self.ensure_connected(&mut guard, timeout, deadline)
            .map_err(|e| anyhow!("embedding host connect: {e}"))?;
        self.cached
            .get()
            .ok_or_else(|| anyhow!("health cache not populated after connect"))
    }

    /// Force the health handshake and return an error if it fails or if the
    /// host reports `ready=false`.
    ///
    /// The `dyn EmbeddingProvider` getters (`device_info`, `accelerated`,
    /// `degraded_reason`, `dimensions`) silently swallow errors by returning
    /// defaults when the health handshake fails. Callers that need a hard gate
    /// — such as the daemon's host-path init — should call `ensure_ready()`
    /// before promoting the provider to `Arc<dyn EmbeddingProvider>`, so that
    /// a host that accepts connections but can't answer health (or reports
    /// `ready=false`) is routed to `publish_unavailable` rather than `Ready`.
    pub fn ensure_ready(&self) -> Result<()> {
        self.get_cached().map(|_| ())
    }
}

/// Returns `true` if the I/O error kind indicates the peer closed or reset
/// the connection.
fn is_connection_dropped(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
    )
}

fn is_valid_sha256_digest(digest: &str) -> bool {
    digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit())
}

// ---------------------------------------------------------------------------
// EmbeddingProvider impl
// ---------------------------------------------------------------------------

impl EmbeddingProvider for RpcEmbeddingProvider {
    fn embed_query(&self, text: &str, budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        budget.check_budget()?;
        let remaining = budget.remaining_time();
        if remaining.is_zero() {
            bail!("embedding request deadline exceeded");
        }
        let result: EmbedQueryResult = self.send_request(
            "embed_query",
            EmbedQueryRequest {
                text: text.to_string(),
                remaining_budget_ms: Some(remaining.as_millis() as u64),
            },
            budget,
        )?;
        validate_query_response(&result, self.dimensions())?;
        budget.check_budget()?;
        Ok(result.vector)
    }

    fn embed_batch(
        &self,
        texts: &[String],
        budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        budget.check_budget()?;
        let remaining = budget.remaining_time();
        if remaining.is_zero() {
            bail!("embedding request deadline exceeded");
        }
        let count = texts.len();
        let result: EmbedBatchResult = self.send_request(
            "embed_batch",
            EmbedBatchRequest {
                texts: texts.to_vec(),
                remaining_budget_ms: Some(remaining.as_millis() as u64),
            },
            budget,
        )?;
        validate_batch_response(&result, count, self.dimensions())?;
        budget.check_budget()?;
        Ok(result.vectors)
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        let cached = self.get_cached()?;
        let health = &cached.health;
        let weights_sha256 = match health.model_sha256.as_deref() {
            Some(sha) if is_valid_sha256_digest(sha) => sha.to_ascii_lowercase(),
            _ => bail!("IdentityUnavailable"),
        };
        let pooling = match health.pooling.as_deref() {
            Some(p) if !p.trim().is_empty() => p.to_string(),
            _ => bail!("IdentityUnavailable: missing pooling"),
        };
        let normalization = match health.normalization.as_deref() {
            Some(n) if !n.trim().is_empty() => n.to_string(),
            _ => bail!("IdentityUnavailable: missing normalization"),
        };
        let instruction_policy = match health.instruction_policy_version {
            Some(v) => format!("v{v}"),
            None => bail!("IdentityUnavailable: missing instruction_policy_version"),
        };
        let model_id = health
            .model_id
            .as_ref()
            .unwrap_or(&cached.device_info.model_name)
            .clone();
        let runtime_build = health
            .llama_cpp_build
            .as_ref()
            .or(health.runtime.as_ref())
            .unwrap_or(&cached.device_info.runtime)
            .clone();
        let identity = EncoderIdentity {
            schema: 1,
            model_id,
            weights_sha256,
            dimensions: cached.dimensions,
            pooling,
            normalization,
            instruction_policy,
            text_format: 1,
            runtime_build,
        };
        identity.validate()?;
        Ok(identity)
    }

    fn dimensions(&self) -> usize {
        self.get_cached().map(|c| c.dimensions).unwrap_or(0)
    }

    fn device_info(&self) -> DeviceInfo {
        self.get_cached()
            .map(|c| c.device_info.clone())
            .unwrap_or(DeviceInfo {
                runtime: "rpc".to_string(),
                device: "unknown".to_string(),
                model_name: "unknown".to_string(),
                dimensions: 0,
            })
    }

    fn accelerated(&self) -> Option<bool> {
        self.get_cached().ok()?.accelerated
    }

    fn degraded_reason(&self) -> Option<String> {
        self.get_cached().ok()?.degraded_reason.clone()
    }

    fn health_check(&self, _budget: &EmbeddingRequestBudget) -> Result<()> {
        self.ensure_ready().map_err(Into::into)
    }

    fn shutdown(&self) {
        if let Ok(mut guard) = self.conn.lock() {
            *guard = None;
        }
    }

    fn wait_for_exit(&self, _timeout: Duration) -> bool {
        // The RPC client does not own the host process; caller is responsible.
        true
    }
}
