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
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use julie_core::embeddings_contract::{DeviceInfo, EmbeddingProvider};

use super::host_transport::{HostAddress, HostClientConn};
use super::sidecar_protocol::{
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult,
    RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
    validate_batch_response, validate_health_response, validate_query_response,
    validate_response_envelope,
};

// ---------------------------------------------------------------------------
// Internal state types
// ---------------------------------------------------------------------------

/// Active connection + per-connection sequential request-id counter.
struct ConnInner {
    conn: HostClientConn,
    request_seq: u64,
}

impl ConnInner {
    fn next_request_id(&mut self) -> String {
        self.request_seq = self.request_seq.wrapping_add(1);
        format!("rpc-{}", self.request_seq)
    }
}

/// Dimensions, device info, and acceleration state cached from the first
/// health round-trip. Written once via [`OnceLock`]; never mutated.
#[derive(Clone)]
struct CachedHealth {
    dimensions: usize,
    device_info: DeviceInfo,
    accelerated: Option<bool>,
    degraded_reason: Option<String>,
}

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
        guard: &mut Option<ConnInner>,
        addr: &HostAddress,
        cached: &OnceLock<CachedHealth>,
    ) -> io::Result<()> {
        if guard.is_some() {
            return Ok(());
        }
        let new_conn = HostClientConn::connect(addr)?;
        let mut inner = ConnInner {
            conn: new_conn,
            request_seq: 0,
        };
        let health = Self::do_health_handshake(&mut inner)?;
        // Only set the cache on the first successful connect; later reconnects
        // are expected to return the same model/dims, so we keep the first value.
        cached.get_or_init(|| {
            let dims = health.dims.unwrap_or(0);
            CachedHealth {
                dimensions: dims,
                device_info: DeviceInfo {
                    runtime: health.runtime.unwrap_or_else(|| "rpc".to_string()),
                    device: health.device.unwrap_or_else(|| "unknown".to_string()),
                    model_name: health.model_id.unwrap_or_else(|| "unknown".to_string()),
                    dimensions: dims,
                },
                accelerated: health.accelerated,
                degraded_reason: health.degraded_reason,
            }
        });
        *guard = Some(inner);
        Ok(())
    }

    /// Send a `health` request on a freshly opened connection and return the
    /// validated [`HealthResult`].
    fn do_health_handshake(inner: &mut ConnInner) -> io::Result<HealthResult> {
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
        let resp_line = inner.conn.round_trip(&line)?;
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
        let health = resp.result.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "health response missing result")
        })?;
        validate_health_response(&health)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        if !health.ready {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "embedding host reported not ready",
            ));
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
        let resp_line = inner.conn.round_trip(&line)?;
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
    ) -> Result<R> {
        let params_val = serde_json::to_value(params)
            .map_err(|e| anyhow!("failed to serialize {method} params: {e}"))?;
        let mut guard = self
            .conn
            .lock()
            .map_err(|_| anyhow!("RpcEmbeddingProvider: mutex poisoned"))?;

        Self::ensure_connected(&mut guard, &self.addr, &self.cached)
            .map_err(|e| anyhow!("embedding host connect: {e}"))?;

        let (resp_line, request_id) = match Self::attempt_once(&mut guard, method, &params_val) {
            Ok(pair) => pair,
            Err(e) if is_connection_dropped(&e) => {
                // Drop the dead connection and reconnect exactly once.
                *guard = None;
                Self::ensure_connected(&mut guard, &self.addr, &self.cached)
                    .map_err(|e| anyhow!("embedding host reconnect: {e}"))?;
                Self::attempt_once(&mut guard, method, &params_val)
                    .map_err(|e| anyhow!("{method} failed after reconnect: {e}"))?
            }
            Err(e) => return Err(anyhow!("{method} io error: {e}")),
        };

        Self::parse_response::<R>(&resp_line, method, &request_id)
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
        Self::ensure_connected(&mut guard, &self.addr, &self.cached)
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

// ---------------------------------------------------------------------------
// EmbeddingProvider impl
// ---------------------------------------------------------------------------

impl EmbeddingProvider for RpcEmbeddingProvider {
    fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let result: EmbedQueryResult = self.send_request(
            "embed_query",
            EmbedQueryRequest {
                text: text.to_string(),
            },
        )?;
        validate_query_response(&result, self.dimensions())?;
        Ok(result.vector)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let count = texts.len();
        let result: EmbedBatchResult = self.send_request(
            "embed_batch",
            EmbedBatchRequest {
                texts: texts.to_vec(),
            },
        )?;
        validate_batch_response(&result, count, self.dimensions())?;
        Ok(result.vectors)
    }

    fn dimensions(&self) -> usize {
        self.get_cached().map(|c| c.dimensions).unwrap_or(0)
    }

    fn device_info(&self) -> DeviceInfo {
        self.get_cached()
            .map(|c| c.device_info.clone())
            .unwrap_or_else(|_| DeviceInfo {
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
