//! Native semantic qualification record schema, validation, and artifact discovery.

use std::path::{Path, PathBuf};

use chrono::DateTime;
use julie_core::EncoderIdentity;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Canonical schema identifier for native qualification artifacts.
pub const QUALIFICATION_SCHEMA_V1: &str = "julie-native-qualification-v1";

/// Optional operational and benchmark metrics captured during qualification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualificationMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prepare_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_rss_bytes: Option<u64>,
}

/// Machine-readable qualification record certifying a native semantic runtime installation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeQualificationRecord {
    /// Schema version string, must be `julie-native-qualification-v1`.
    pub schema: String,
    /// 40-character hex commit SHA of the Julie repository.
    pub julie_source_sha: String,
    /// 40-character hex commit SHA of the julie-semantic-sidecar repository.
    pub sidecar_source_sha: String,
    /// 64-character lowercase hex SHA-256 digest of the sidecar binary executable.
    pub executable_sha256: String,
    /// Complete cryptographic identity of the active embedding model.
    pub encoder_identity: EncoderIdentity,
    /// Active compute backend (e.g. "native", "cpu", "metal", "vulkan", "cuda").
    #[serde(alias = "resolved_backend")]
    pub backend: String,
    /// Hardware device string reported by runtime (e.g. "cpu", "Apple M1").
    pub device: String,
    /// Whether hardware acceleration is active.
    pub accelerated: bool,
    /// Whether a physical GPU acceleration lane was verified on host hardware.
    #[serde(default)]
    pub gpu_lane_verified: bool,
    /// Git commit SHA or revision identifier of the qualified corpus.
    pub corpus_commit: String,
    /// SQLite database canonical symbol revision.
    pub canonical_revision: i64,
    /// Vector generation source revision.
    pub vector_revision: i64,
    /// Count of symbols eligible for semantic vector embedding in the corpus.
    pub eligible_symbols: usize,
    /// Count of symbols successfully embedded in the active vector generation.
    pub embedded_symbols: usize,
    /// Qualification timestamp in ISO 8601 / RFC 3339 format.
    #[serde(alias = "qualified_at", alias = "qualification_timestamp")]
    pub timestamp: String,
    /// Boolean qualification verdict set to true once all invariants pass.
    #[serde(default)]
    pub qualified: bool,
    /// Human-readable explanation when qualification is marked false or stale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disqualification_reason: Option<String>,
    /// Explicit list of hardware lanes that remain unverified on this host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unverified_lanes: Option<Vec<String>>,
    /// Optional benchmark and execution metrics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<QualificationMetrics>,
}

/// Typed validation errors for qualification record verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum QualificationValidationError {
    #[error("missing or invalid schema: expected '{expected}', got '{actual}'")]
    InvalidSchema {
        expected: &'static str,
        actual: String,
    },

    #[error("record explicitly marked as unqualified")]
    UnqualifiedRecord,

    #[error("invalid julie_source_sha: expected 40-char hex digest, got '{0}'")]
    InvalidJulieSourceSha(String),

    #[error("invalid sidecar_source_sha: expected 40-char hex digest, got '{0}'")]
    InvalidSidecarSourceSha(String),

    #[error("invalid executable_sha256: expected 64-char lowercase hex digest, got '{0}'")]
    InvalidExecutableSha256(String),

    #[error("invalid encoder_identity: {0}")]
    InvalidEncoderIdentity(String),

    #[error("backend 'python'/'sidecar' is forbidden in native semantic qualification")]
    PythonBackendForbidden,

    #[error("backend must not be empty")]
    EmptyBackend,

    #[error(
        "unknown or unsupported backend '{0}'; expected known backend ('native', 'cpu', 'metal', 'vulkan', 'cuda')"
    )]
    UnknownBackend(String),

    #[error(
        "invalid revision: revisions must be non-negative, got canonical={canonical}, vector={vector}"
    )]
    NegativeRevision { canonical: i64, vector: i64 },

    #[error("device must not be empty")]
    EmptyDevice,

    #[error(
        "inconsistent accelerator configuration: backend '{backend}' with device '{device}' cannot claim accelerated={accelerated}, gpu_lane_verified={gpu_lane_verified}"
    )]
    InconsistentAcceleration {
        backend: String,
        device: String,
        accelerated: bool,
        gpu_lane_verified: bool,
    },

    #[error("corpus_commit must not be empty")]
    EmptyCorpusCommit,

    #[error("revision mismatch: canonical_revision ({canonical}) != vector_revision ({vector})")]
    RevisionMismatch { canonical: i64, vector: i64 },

    #[error("incomplete vector coverage: {embedded} embedded of {eligible} eligible symbols")]
    IncompleteCoverage { eligible: usize, embedded: usize },

    #[error("invalid qualification timestamp: '{0}'")]
    InvalidTimestamp(String),

    #[error("deserialization error: {0}")]
    Deserialization(String),
}

/// Validates a native qualification artifact against schema and consistency invariants.
///
/// Invariants enforced:
/// 1. Schema must equal `julie-native-qualification-v1`.
/// 2. `qualified` must not be explicitly false.
/// 3. `julie_source_sha` and `sidecar_source_sha` must be 40-character hex strings.
/// 4. `executable_sha256` must be a 64-character lowercase hex string.
/// 5. `encoder_identity` must pass `EncoderIdentity::validate()` and match pinned model specs.
/// 6. `backend` cannot be "python" or "sidecar", and cannot be empty.
/// 7. `device` must not be empty.
/// 8. Device and accelerator consistency: CPU cannot claim acceleration or verified GPU lane.
/// 9. `corpus_commit` must not be empty.
/// 10. Revisions must match: `canonical_revision == vector_revision`.
/// 11. Vector coverage must be complete: `embedded_symbols >= eligible_symbols`.
/// 12. `timestamp` must parse as a valid RFC 3339 date-time.
pub fn validate_native_qualification(
    value: &Value,
) -> Result<NativeQualificationRecord, QualificationValidationError> {
    let schema_str = value
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if schema_str != QUALIFICATION_SCHEMA_V1 {
        return Err(QualificationValidationError::InvalidSchema {
            expected: QUALIFICATION_SCHEMA_V1,
            actual: schema_str.to_string(),
        });
    }

    if value.get("qualified") == Some(&Value::Bool(false)) {
        return Err(QualificationValidationError::UnqualifiedRecord);
    }

    let mut record: NativeQualificationRecord = serde_json::from_value(value.clone())
        .map_err(|e| QualificationValidationError::Deserialization(e.to_string()))?;

    validate_hex_digest(&record.julie_source_sha, 40, false).map_err(|_| {
        QualificationValidationError::InvalidJulieSourceSha(record.julie_source_sha.clone())
    })?;

    validate_hex_digest(&record.sidecar_source_sha, 40, false).map_err(|_| {
        QualificationValidationError::InvalidSidecarSourceSha(record.sidecar_source_sha.clone())
    })?;

    validate_hex_digest(&record.executable_sha256, 64, true).map_err(|_| {
        QualificationValidationError::InvalidExecutableSha256(record.executable_sha256.clone())
    })?;

    record
        .encoder_identity
        .validate()
        .map_err(|e| QualificationValidationError::InvalidEncoderIdentity(e.to_string()))?;

    // Validate pinned model dimensions and pooling if recognized
    if record.encoder_identity.model_id == "bge-small-en-v1.5-f32" {
        if record.encoder_identity.dimensions != 384 {
            return Err(QualificationValidationError::InvalidEncoderIdentity(
                format!(
                    "bge-small-en-v1.5-f32 expected 384 dimensions, got {}",
                    record.encoder_identity.dimensions
                ),
            ));
        }
        if record.encoder_identity.pooling != "cls" {
            return Err(QualificationValidationError::InvalidEncoderIdentity(
                format!(
                    "bge-small-en-v1.5-f32 expected 'cls' pooling, got '{}'",
                    record.encoder_identity.pooling
                ),
            ));
        }
    } else if record.encoder_identity.model_id == "qwen3-0.6b-f16" {
        if record.encoder_identity.dimensions != 512 {
            return Err(QualificationValidationError::InvalidEncoderIdentity(
                format!(
                    "qwen3-0.6b-f16 expected 512 served dimensions, got {}",
                    record.encoder_identity.dimensions
                ),
            ));
        }
        if record.encoder_identity.pooling != "last" {
            return Err(QualificationValidationError::InvalidEncoderIdentity(
                format!(
                    "qwen3-0.6b-f16 expected 'last' pooling, got '{}'",
                    record.encoder_identity.pooling
                ),
            ));
        }
    }

    let backend_norm = record.backend.trim().to_lowercase();
    if backend_norm.is_empty() {
        return Err(QualificationValidationError::EmptyBackend);
    }
    if backend_norm == "python" || backend_norm == "sidecar" {
        return Err(QualificationValidationError::PythonBackendForbidden);
    }
    const ALLOWED_BACKENDS: &[&str] = &["native", "cpu", "metal", "vulkan", "cuda", "python"];
    if !ALLOWED_BACKENDS.contains(&backend_norm.as_str()) {
        return Err(QualificationValidationError::UnknownBackend(
            record.backend.clone(),
        ));
    }

    let device_norm = record.device.trim().to_lowercase();
    if device_norm.is_empty() {
        return Err(QualificationValidationError::EmptyDevice);
    }

    let is_cpu = device_norm == "cpu" || backend_norm == "cpu";
    if is_cpu && record.accelerated {
        return Err(QualificationValidationError::InconsistentAcceleration {
            backend: record.backend.clone(),
            device: record.device.clone(),
            accelerated: record.accelerated,
            gpu_lane_verified: record.gpu_lane_verified,
        });
    }
    if is_cpu && record.gpu_lane_verified {
        return Err(QualificationValidationError::InconsistentAcceleration {
            backend: record.backend.clone(),
            device: record.device.clone(),
            accelerated: record.accelerated,
            gpu_lane_verified: record.gpu_lane_verified,
        });
    }
    if record.accelerated && (device_norm == "cpu" || device_norm == "unavailable") {
        return Err(QualificationValidationError::InconsistentAcceleration {
            backend: record.backend.clone(),
            device: record.device.clone(),
            accelerated: record.accelerated,
            gpu_lane_verified: record.gpu_lane_verified,
        });
    }
    if record.gpu_lane_verified
        && (device_norm == "cpu" || device_norm == "unavailable" || !record.accelerated)
    {
        return Err(QualificationValidationError::InconsistentAcceleration {
            backend: record.backend.clone(),
            device: record.device.clone(),
            accelerated: record.accelerated,
            gpu_lane_verified: record.gpu_lane_verified,
        });
    }

    if record.corpus_commit.trim().is_empty() {
        return Err(QualificationValidationError::EmptyCorpusCommit);
    }

    if record.canonical_revision < 0 || record.vector_revision < 0 {
        return Err(QualificationValidationError::NegativeRevision {
            canonical: record.canonical_revision,
            vector: record.vector_revision,
        });
    }

    if record.canonical_revision != record.vector_revision {
        return Err(QualificationValidationError::RevisionMismatch {
            canonical: record.canonical_revision,
            vector: record.vector_revision,
        });
    }

    if record.embedded_symbols < record.eligible_symbols {
        return Err(QualificationValidationError::IncompleteCoverage {
            eligible: record.eligible_symbols,
            embedded: record.embedded_symbols,
        });
    }

    if DateTime::parse_from_rfc3339(&record.timestamp).is_err() {
        return Err(QualificationValidationError::InvalidTimestamp(
            record.timestamp.clone(),
        ));
    }

    record.qualified = true;
    record.disqualification_reason = None;
    Ok(record)
}

fn validate_hex_digest(val: &str, expected_len: usize, require_lowercase: bool) -> Result<(), ()> {
    if val.len() != expected_len {
        return Err(());
    }
    for c in val.chars() {
        if !c.is_ascii_hexdigit() {
            return Err(());
        }
        if require_lowercase && c.is_ascii_uppercase() {
            return Err(());
        }
    }
    Ok(())
}

/// Canonical path for a workspace qualification artifact: `<workspace_root>/.julie/qualification.json`.
pub fn workspace_qualification_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".julie").join("qualification.json")
}

/// Reads and validates a qualification artifact from a workspace directory.
pub fn load_workspace_qualification(workspace_root: &Path) -> Option<NativeQualificationRecord> {
    let path = workspace_qualification_path(workspace_root);
    if !path.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;
    validate_native_qualification(&value).ok()
}

mod runtime;

pub use runtime::{
    ActiveProviderFacts, WorkspaceQualificationMismatch,
    validate_qualification_against_running_runtime, validate_qualification_against_workspace,
};
