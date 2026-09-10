//! Health check verification and cryptographic EncoderIdentity construction.

use anyhow::{Result, bail};

use julie_core::embeddings_contract::{DeviceInfo, EmbeddingRequestBudget, EncoderIdentity};

use crate::embeddings::native::child::SidecarChild;
use crate::embeddings::sidecar_protocol::HealthResult;

/// Validates a sidecar child's `HealthResult` and builds verified `EncoderIdentity` and `DeviceInfo`.
pub fn validate_native_health(
    health: &HealthResult,
    expected_model_id: Option<&str>,
) -> Result<(EncoderIdentity, DeviceInfo)> {
    if !health.ready {
        if health.degraded_reason.as_deref() == Some("model_not_prepared") {
            bail!("MODEL_NOT_PREPARED: sidecar reports model is not prepared");
        }
        bail!(
            "sidecar unready: {}",
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
                "model_id mismatch: sidecar reported model '{model_id}', expected manifest model '{expected}'"
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

/// Issues a health request to `child` and returns the health report, verified identity, and device info.
pub fn query_and_validate_health(
    child: &mut SidecarChild,
    budget: &EmbeddingRequestBudget,
) -> Result<(HealthResult, EncoderIdentity, DeviceInfo)> {
    let health = child.health(budget)?;
    let (identity, device_info) = validate_native_health(&health, None)?;
    Ok((health, identity, device_info))
}
