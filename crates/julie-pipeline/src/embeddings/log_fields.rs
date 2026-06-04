//! Embedding runtime log-fields helpers.
//!
//! Relocated from `src/workspace/mod.rs` so the embeddings module owns its
//! own telemetry types rather than reaching up into workspace.

use crate::embeddings::{DeviceInfo, EmbeddingRuntimeStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingRuntimeLogFields {
    pub requested_backend: String,
    pub resolved_backend: String,
    pub runtime: String,
    pub device: String,
    pub accelerated: bool,
    pub degraded_reason: String,
    pub telemetry_confidence: String,
    pub strict_mode: bool,
    pub fallback_used: bool,
}

fn embedding_telemetry_confidence(provider_info: Option<&DeviceInfo>) -> &'static str {
    let Some(info) = provider_info else {
        return "low";
    };

    let runtime = info.runtime.trim().to_ascii_lowercase();
    let device = info.device.trim().to_ascii_lowercase();
    if runtime.is_empty()
        || device.is_empty()
        || runtime.contains("unknown")
        || runtime.contains("unavailable")
        || device.contains("unknown")
        || device.contains("unavailable")
    {
        "low"
    } else {
        "high"
    }
}

pub fn build_embedding_runtime_log_fields(
    status: &EmbeddingRuntimeStatus,
    provider_info: Option<&DeviceInfo>,
    strict_mode: bool,
    fallback_used: bool,
) -> EmbeddingRuntimeLogFields {
    EmbeddingRuntimeLogFields {
        requested_backend: status.requested_backend.as_str().to_string(),
        resolved_backend: status.resolved_backend.as_str().to_string(),
        runtime: provider_info
            .map(|info| info.runtime.clone())
            .unwrap_or_else(|| "unavailable".to_string()),
        device: provider_info
            .map(|info| info.device.clone())
            .unwrap_or_else(|| "unavailable".to_string()),
        accelerated: status.accelerated,
        degraded_reason: status
            .degraded_reason
            .clone()
            .unwrap_or_else(|| "none".to_string()),
        telemetry_confidence: embedding_telemetry_confidence(provider_info).to_string(),
        strict_mode,
        fallback_used,
    }
}
