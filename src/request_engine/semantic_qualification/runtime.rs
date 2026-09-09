//! Live runtime qualification reconciliation and workspace state validation.

use julie_core::EncoderIdentity;
use julie_core::database::EmbeddingGeneration;
use thiserror::Error;

use super::NativeQualificationRecord;

/// Mismatch reasons when verifying a qualification record against live workspace state.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum WorkspaceQualificationMismatch {
    #[error(
        "canonical revision mismatch: record has {record_rev}, workspace DB has {workspace_rev}"
    )]
    CanonicalRevisionMismatch { record_rev: i64, workspace_rev: i64 },

    #[error("no ready embedding generation found in workspace database")]
    NoReadyGeneration,

    #[error("vector revision mismatch: record has {record_rev}, ready generation has {gen_rev}")]
    VectorRevisionMismatch { record_rev: i64, gen_rev: i64 },

    #[error(
        "symbol coverage mismatch: record has {rec_embedded}/{rec_eligible}, ready generation has {gen_embedded}/{gen_eligible}"
    )]
    CoverageMismatch {
        rec_eligible: usize,
        rec_embedded: usize,
        gen_eligible: usize,
        gen_embedded: usize,
    },

    #[error(
        "encoder key mismatch: record encoder key '{record_key}' does not match ready generation '{gen_key}'"
    )]
    EncoderKeyMismatch { record_key: String, gen_key: String },

    #[error(
        "executable SHA mismatch: record has '{record_sha}', active runtime has '{active_sha}'"
    )]
    ExecutableShaMismatch {
        record_sha: String,
        active_sha: String,
    },

    #[error("invalid encoder identity storage key: {0}")]
    InvalidStorageKey(String),

    #[error(
        "backend mismatch: record has '{record_backend}', active runtime has '{active_backend}'"
    )]
    BackendMismatch {
        record_backend: String,
        active_backend: String,
    },

    #[error(
        "active encoder mismatch: record has '{record_model}', active provider has '{active_model}'"
    )]
    ActiveEncoderMismatch {
        record_model: String,
        active_model: String,
    },

    #[error("device mismatch: record has '{record_device}', active provider has '{active_device}'")]
    DeviceMismatch {
        record_device: String,
        active_device: String,
    },

    #[error("acceleration mismatch: record has {record_accel}, active provider has {active_accel}")]
    AccelerationMismatch {
        record_accel: bool,
        active_accel: bool,
    },

    #[error(
        "missing running sidecar executable SHA: active native provider did not report an executable digest"
    )]
    MissingRunningSidecarSha,

    #[error("missing live evidence: active provider did not report {0}")]
    MissingLiveEvidence(String),
}

/// Observed runtime properties from an active embedding provider and runtime plane.
pub struct ActiveProviderFacts<'a> {
    pub backend: &'a str,
    pub encoder_identity: Option<&'a EncoderIdentity>,
    pub device: Option<&'a str>,
    pub accelerated: Option<bool>,
    pub running_executable_sha: Option<&'a str>,
}

/// Validates a qualification record against the live workspace database and runtime state.
pub fn validate_qualification_against_workspace(
    record: &NativeQualificationRecord,
    current_canonical_rev: i64,
    ready_gen: Option<&EmbeddingGeneration>,
    active_sidecar_sha: Option<&str>,
) -> Result<(), WorkspaceQualificationMismatch> {
    if record.canonical_revision != current_canonical_rev {
        return Err(WorkspaceQualificationMismatch::CanonicalRevisionMismatch {
            record_rev: record.canonical_revision,
            workspace_rev: current_canonical_rev,
        });
    }

    let ready_generation = ready_gen.ok_or(WorkspaceQualificationMismatch::NoReadyGeneration)?;

    if record.vector_revision != ready_generation.source_revision {
        return Err(WorkspaceQualificationMismatch::VectorRevisionMismatch {
            record_rev: record.vector_revision,
            gen_rev: ready_generation.source_revision,
        });
    }

    if record.eligible_symbols != ready_generation.eligible_symbols
        || record.embedded_symbols != ready_generation.embedded_symbols
    {
        return Err(WorkspaceQualificationMismatch::CoverageMismatch {
            rec_eligible: record.eligible_symbols,
            rec_embedded: record.embedded_symbols,
            gen_eligible: ready_generation.eligible_symbols,
            gen_embedded: ready_generation.embedded_symbols,
        });
    }

    let record_key = record
        .encoder_identity
        .storage_key()
        .map_err(|e| WorkspaceQualificationMismatch::InvalidStorageKey(e.to_string()))?;

    if record_key != ready_generation.encoder_key {
        return Err(WorkspaceQualificationMismatch::EncoderKeyMismatch {
            record_key,
            gen_key: ready_generation.encoder_key.clone(),
        });
    }

    if let Some(active_sha) = active_sidecar_sha {
        if !active_sha.is_empty() && !record.executable_sha256.eq_ignore_ascii_case(active_sha) {
            return Err(WorkspaceQualificationMismatch::ExecutableShaMismatch {
                record_sha: record.executable_sha256.clone(),
                active_sha: active_sha.to_string(),
            });
        }
    }

    Ok(())
}

/// Validates a qualification record against the live workspace DB, active runtime backend,
/// active encoder identity, device, acceleration state, and reported executable digest.
pub fn validate_qualification_against_running_runtime(
    record: &NativeQualificationRecord,
    current_canonical_rev: i64,
    ready_gen: Option<&EmbeddingGeneration>,
    active: &ActiveProviderFacts<'_>,
) -> Result<(), WorkspaceQualificationMismatch> {
    // 1. Verify workspace and DB state via validate_qualification_against_workspace
    validate_qualification_against_workspace(
        record,
        current_canonical_rev,
        ready_gen,
        active.running_executable_sha,
    )?;

    // 2. Verify active.backend == "native" if record is native; reject backend mismatch
    if active.backend != "native" {
        return Err(WorkspaceQualificationMismatch::BackendMismatch {
            record_backend: record.backend.clone(),
            active_backend: active.backend.to_string(),
        });
    }

    // 3. Verify active.encoder_identity.storage_key() == record.encoder_identity.storage_key(); reject active encoder mismatch
    let active_id = active.encoder_identity.ok_or_else(|| {
        WorkspaceQualificationMismatch::MissingLiveEvidence("encoder identity".to_string())
    })?;
    let active_key = active_id
        .storage_key()
        .map_err(|e| WorkspaceQualificationMismatch::InvalidStorageKey(e.to_string()))?;
    let record_key = record
        .encoder_identity
        .storage_key()
        .map_err(|e| WorkspaceQualificationMismatch::InvalidStorageKey(e.to_string()))?;
    if active_key != record_key {
        return Err(WorkspaceQualificationMismatch::ActiveEncoderMismatch {
            record_model: record.encoder_identity.model_id.clone(),
            active_model: active_id.model_id.clone(),
        });
    }

    // 4. Verify active.device.eq_ignore_ascii_case(&record.device); reject device mismatch
    let active_dev = active
        .device
        .ok_or_else(|| WorkspaceQualificationMismatch::MissingLiveEvidence("device".to_string()))?;
    if !record.device.eq_ignore_ascii_case(active_dev) {
        return Err(WorkspaceQualificationMismatch::DeviceMismatch {
            record_device: record.device.clone(),
            active_device: active_dev.to_string(),
        });
    }

    // 5. Verify active.accelerated == record.accelerated; reject acceleration mismatch
    let active_accel = active.accelerated.ok_or_else(|| {
        WorkspaceQualificationMismatch::MissingLiveEvidence("acceleration".to_string())
    })?;
    if record.accelerated != active_accel {
        return Err(WorkspaceQualificationMismatch::AccelerationMismatch {
            record_accel: record.accelerated,
            active_accel,
        });
    }

    // 6. Verify active.running_executable_sha is present and matches record.executable_sha256; reject missing or mismatched executable SHA
    let running_sha = active
        .running_executable_sha
        .ok_or(WorkspaceQualificationMismatch::MissingRunningSidecarSha)?;
    if running_sha.is_empty() {
        return Err(WorkspaceQualificationMismatch::MissingRunningSidecarSha);
    }
    if !record.executable_sha256.eq_ignore_ascii_case(running_sha) {
        return Err(WorkspaceQualificationMismatch::ExecutableShaMismatch {
            record_sha: record.executable_sha256.clone(),
            active_sha: running_sha.to_string(),
        });
    }

    Ok(())
}
