//! Sidecar IPC protocol contracts for embedding requests/responses.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const SIDECAR_PROTOCOL_SCHEMA: &str = "julie.embedding.sidecar";
pub const SIDECAR_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RequestEnvelope<T> {
    pub schema: String,
    pub version: u32,
    #[serde(alias = "id")]
    pub request_id: String,
    pub method: String,
    pub params: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResponseEnvelope<T> {
    pub schema: String,
    pub version: u32,
    #[serde(alias = "id")]
    pub request_id: String,
    pub result: Option<T>,
    pub error: Option<ProtocolError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbedQueryRequest {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbedBatchRequest {
    pub texts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbedQueryResult {
    pub dims: usize,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbedBatchResult {
    pub dims: usize,
    pub vectors: Vec<Vec<f32>>,
}

pub fn validate_response_envelope<T>(
    env: &ResponseEnvelope<T>,
    expected_request_id: &str,
) -> Result<()> {
    if env.schema != SIDECAR_PROTOCOL_SCHEMA {
        bail!(
            "sidecar response schema mismatch: expected '{}', got '{}'",
            SIDECAR_PROTOCOL_SCHEMA,
            env.schema
        );
    }

    if env.version != SIDECAR_PROTOCOL_VERSION {
        bail!(
            "sidecar response version mismatch: expected {}, got {}",
            SIDECAR_PROTOCOL_VERSION,
            env.version
        );
    }

    if env.request_id != expected_request_id {
        bail!(
            "sidecar response request_id mismatch: expected '{}', got '{}'",
            expected_request_id,
            env.request_id
        );
    }

    let has_result = env.result.is_some();
    let has_error = env.error.is_some();
    if has_result == has_error {
        bail!(
            "sidecar response envelope invariant violation: exactly one of result or error must be set"
        );
    }

    Ok(())
}

pub fn validate_query_response(resp: &EmbedQueryResult, expected_dims: usize) -> Result<()> {
    if resp.dims != expected_dims {
        bail!(
            "sidecar query embedding dimension mismatch: expected {}, got {}",
            expected_dims,
            resp.dims
        );
    }

    if resp.vector.len() != expected_dims {
        bail!(
            "sidecar query embedding vector length mismatch: expected {}, got {}",
            expected_dims,
            resp.vector.len()
        );
    }

    Ok(())
}

pub fn validate_batch_response(
    resp: &EmbedBatchResult,
    expected_count: usize,
    expected_dims: usize,
) -> Result<()> {
    if resp.dims != expected_dims {
        bail!(
            "sidecar batch embedding dimension mismatch: expected {}, got {}",
            expected_dims,
            resp.dims
        );
    }

    if resp.vectors.len() != expected_count {
        bail!(
            "sidecar batch embedding count mismatch: expected {}, got {}",
            expected_count,
            resp.vectors.len()
        );
    }

    if let Some((index, actual)) =
        resp.vectors.iter().enumerate().find_map(|(idx, vector)| {
            (vector.len() != expected_dims).then_some((idx, vector.len()))
        })
    {
        bail!(
            "sidecar batch embedding vector length mismatch at index {}: expected {}, got {}",
            index,
            expected_dims,
            actual
        );
    }

    Ok(())
}
