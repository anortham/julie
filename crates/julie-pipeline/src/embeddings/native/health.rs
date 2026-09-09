//! Health check verification and cryptographic EncoderIdentity construction.

use anyhow::{Result, bail};

use julie_core::embeddings_contract::{DeviceInfo, EmbeddingRequestBudget, EncoderIdentity};

use crate::embeddings::native::client::{NativeClientConn, decode_native_health_reply};
use crate::embeddings::sidecar_protocol::{
    HealthResult, RequestEnvelope, SIDECAR_PROTOCOL_SCHEMA, SIDECAR_PROTOCOL_VERSION,
};

/// Validates a broker's `HealthResult` and builds verified `EncoderIdentity` and `DeviceInfo`.
pub fn validate_native_health(
    health: &HealthResult,
    expected_model_id: Option<&str>,
) -> Result<(EncoderIdentity, DeviceInfo)> {
    if !health.ready {
        if health.degraded_reason.as_deref() == Some("model_not_prepared") {
            bail!("MODEL_NOT_PREPARED: sidecar reports model is not prepared");
        }
        bail!(
            "native broker unready: {}",
            health
                .degraded_reason
                .as_deref()
                .unwrap_or("unknown reason")
        );
    }

    let model_id = health
        .model_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing model_id in sidecar health response"))?
        .to_string();

    if let Some(expected) = expected_model_id {
        if model_id != expected {
            bail!(
                "model_id mismatch: broker reported model '{model_id}', expected manifest model '{expected}'"
            );
        }
    }

    let weights_sha256 = health
        .model_sha256
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing model_sha256 in sidecar health response"))?
        .to_string();

    let dimensions = health
        .dims
        .ok_or_else(|| anyhow::anyhow!("missing dims in sidecar health response"))?;

    let pooling = health
        .pooling
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing pooling in sidecar health response"))?
        .to_string();

    let normalization = health
        .normalization
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing normalization in sidecar health response"))?
        .to_string();

    let instruction_policy = format!(
        "v{}",
        health
            .instruction_policy_version
            .ok_or_else(|| anyhow::anyhow!(
                "missing instruction_policy_version in sidecar health response"
            ))?
    );

    let llama_build = health
        .llama_cpp_build
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing llama_cpp_build in sidecar health response"))?;
    let runtime_build = format!("llama.cpp-{llama_build}");

    let identity = EncoderIdentity {
        schema: 1,
        model_id: model_id.clone(),
        weights_sha256,
        dimensions,
        pooling,
        normalization,
        instruction_policy,
        text_format: 1,
        runtime_build,
    };

    identity.validate()?;
    let _ = identity.storage_key()?;

    let device_info = DeviceInfo {
        runtime: health
            .runtime
            .clone()
            .unwrap_or_else(|| "llama.cpp".to_string()),
        device: health.device.clone().unwrap_or_else(|| "cpu".to_string()),
        model_name: model_id,
        dimensions,
    };

    Ok((identity, device_info))
}

/// Issues a health request over `client` and returns the verified identity, device info, and raw health report.
pub fn query_and_validate_health(
    client: &mut NativeClientConn,
    budget: &EmbeddingRequestBudget,
    expected_model_id: Option<&str>,
) -> Result<(EncoderIdentity, DeviceInfo, HealthResult)> {
    budget.check_budget()?;

    let req_id = "health-init";
    let envelope = RequestEnvelope {
        schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
        version: SIDECAR_PROTOCOL_VERSION,
        request_id: req_id.to_string(),
        method: "health".to_string(),
        params: serde_json::json!({}),
    };

    let req_bytes = serde_json::to_vec(&envelope)?;
    let remaining = budget.remaining_time();
    let resp_bytes = client.round_trip(&req_bytes, Some(remaining))?;

    let health = decode_native_health_reply(&resp_bytes, req_id)?;
    let (identity, device_info) = validate_native_health(&health, expected_model_id)?;

    Ok((identity, device_info, health))
}
