//! Pure reply decoders for native embeddings sidecar.

use anyhow::{Result, bail};

use crate::embeddings::sidecar_protocol::{
    EmbedBatchResult, EmbedQueryResult, HealthResult, ResponseEnvelope, validate_health_response,
    validate_response_envelope,
};

/// Pure reply decoder for single-query embedding responses.
pub fn decode_native_query_reply(
    bytes: &[u8],
    expected_request_id: &str,
    expected_dims: usize,
) -> Result<Vec<f32>> {
    let env: ResponseEnvelope<EmbedQueryResult> = serde_json::from_slice(bytes)
        .map_err(|e| anyhow::anyhow!("invalid json in sidecar reply: {e}"))?;

    validate_response_envelope(&env, expected_request_id)?;

    if let Some(err) = env.error {
        bail!("sidecar error [{}]: {}", err.code, err.message);
    }

    let result = env
        .result
        .ok_or_else(|| anyhow::anyhow!("missing result in sidecar reply"))?;

    if result.dims != expected_dims {
        bail!(
            "dimension mismatch: expected {expected_dims}, got {}",
            result.dims
        );
    }

    if result.vector.len() != expected_dims {
        bail!(
            "vector length mismatch: expected {expected_dims}, got {}",
            result.vector.len()
        );
    }

    for (idx, val) in result.vector.iter().enumerate() {
        if !val.is_finite() {
            bail!("non-finite float at index {idx}: {val}");
        }
    }

    Ok(result.vector)
}

/// Pure reply decoder for batch embedding responses.
pub fn decode_native_batch_reply(
    bytes: &[u8],
    expected_request_id: &str,
    expected_dims: usize,
    expected_count: usize,
) -> Result<Vec<Vec<f32>>> {
    let env: ResponseEnvelope<EmbedBatchResult> = serde_json::from_slice(bytes)
        .map_err(|e| anyhow::anyhow!("invalid json in sidecar reply: {e}"))?;

    validate_response_envelope(&env, expected_request_id)?;

    if let Some(err) = env.error {
        bail!("sidecar error [{}]: {}", err.code, err.message);
    }

    let result = env
        .result
        .ok_or_else(|| anyhow::anyhow!("missing result in sidecar reply"))?;

    if result.dims != expected_dims {
        bail!(
            "dimension mismatch: expected {expected_dims}, got {}",
            result.dims
        );
    }

    if result.vectors.len() != expected_count {
        bail!(
            "batch count mismatch: expected {expected_count}, got {}",
            result.vectors.len()
        );
    }

    for (v_idx, vector) in result.vectors.iter().enumerate() {
        if vector.len() != expected_dims {
            bail!(
                "vector length mismatch at index {v_idx}: expected {expected_dims}, got {}",
                vector.len()
            );
        }
        for (f_idx, val) in vector.iter().enumerate() {
            if !val.is_finite() {
                bail!("non-finite float in vector {v_idx} at index {f_idx}: {val}");
            }
        }
    }

    Ok(result.vectors)
}

/// Pure reply decoder for health check responses.
pub fn decode_native_health_reply(bytes: &[u8], expected_request_id: &str) -> Result<HealthResult> {
    let env: ResponseEnvelope<HealthResult> = serde_json::from_slice(bytes)
        .map_err(|e| anyhow::anyhow!("invalid json in sidecar reply: {e}"))?;

    validate_response_envelope(&env, expected_request_id)?;

    if let Some(err) = env.error {
        bail!("sidecar error [{}]: {}", err.code, err.message);
    }

    let result = env
        .result
        .ok_or_else(|| anyhow::anyhow!("missing result in sidecar reply"))?;

    validate_health_response(&result)?;

    Ok(result)
}
