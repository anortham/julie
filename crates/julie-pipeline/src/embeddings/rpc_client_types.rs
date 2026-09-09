//! Internal state types for `RpcEmbeddingProvider`.

use julie_core::embeddings_contract::DeviceInfo;

use super::host_transport::HostClientConn;
use super::sidecar_protocol::HealthResult;

/// Active connection + per-connection sequential request-id counter.
pub struct ConnInner {
    pub conn: HostClientConn,
    pub request_seq: u64,
}

impl ConnInner {
    pub fn next_request_id(&mut self) -> String {
        self.request_seq = self.request_seq.wrapping_add(1);
        format!("rpc-{}", self.request_seq)
    }
}

/// Dimensions, device info, and acceleration state cached from the first
/// health round-trip. Written once via `OnceLock`; never mutated.
#[derive(Clone)]
pub struct CachedHealth {
    pub health: HealthResult,
    pub dimensions: usize,
    pub device_info: DeviceInfo,
    pub accelerated: Option<bool>,
    pub degraded_reason: Option<String>,
}

impl CachedHealth {
    pub fn from_health(health: HealthResult) -> Self {
        let dimensions = health.dims.unwrap_or(0);
        let device_info = DeviceInfo {
            runtime: health.runtime.clone().unwrap_or_else(|| "rpc".to_string()),
            device: health
                .device
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            model_name: health
                .model_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            dimensions,
        };
        Self {
            accelerated: health.accelerated,
            degraded_reason: health.degraded_reason.clone(),
            dimensions,
            device_info,
            health,
        }
    }
}
