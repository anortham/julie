//! Tests for live semantic qualification reconciliation against running runtime.

use julie_core::database::{EmbeddingGeneration, EmbeddingGenerationStatus};
use serde_json::json;

use crate::request_engine::semantic_qualification::{
    ActiveProviderFacts, QUALIFICATION_SCHEMA_V1, WorkspaceQualificationMismatch,
    validate_native_qualification, validate_qualification_against_running_runtime,
};

fn sample_bge_record() -> crate::request_engine::semantic_qualification::NativeQualificationRecord {
    let val = json!({
        "schema": QUALIFICATION_SCHEMA_V1,
        "julie_source_sha": "5518d49c88e9d0d696f408f0c6e8fd1657513b0a",
        "sidecar_source_sha": "9ed082ba511aa8b10c9e7b47110c3a4dd1e98d59",
        "executable_sha256": "c27b95fb1e975884c11c964814b019618b959b894ab19437ff641eca648ac51d",
        "encoder_identity": {
            "schema": 1,
            "model_id": "bge-small-en-v1.5-f32",
            "weights_sha256": "bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65",
            "dimensions": 384,
            "pooling": "cls",
            "normalization": "l2",
            "instruction_policy": "v1",
            "text_format": 1,
            "runtime_build": "llama.cpp-b3560"
        },
        "backend": "native",
        "device": "cpu",
        "accelerated": false,
        "gpu_lane_verified": false,
        "corpus_commit": "HEAD",
        "canonical_revision": 42,
        "vector_revision": 42,
        "eligible_symbols": 100,
        "embedded_symbols": 100,
        "timestamp": "2026-09-09T02:00:00Z",
        "qualified": true,
        "unverified_lanes": ["metal", "vulkan", "cuda"]
    });
    validate_native_qualification(&val).expect("valid bge record")
}

fn sample_qwen_record() -> crate::request_engine::semantic_qualification::NativeQualificationRecord
{
    let val = json!({
        "schema": QUALIFICATION_SCHEMA_V1,
        "julie_source_sha": "5518d49c88e9d0d696f408f0c6e8fd1657513b0a",
        "sidecar_source_sha": "9ed082ba511aa8b10c9e7b47110c3a4dd1e98d59",
        "executable_sha256": "c27b95fb1e975884c11c964814b019618b959b894ab19437ff641eca648ac51d",
        "encoder_identity": {
            "schema": 1,
            "model_id": "qwen3-0.6b-f16",
            "weights_sha256": "421a27e58d165478cc7acb984a688c2aa41404968b0203e7cd743ece44c54340",
            "dimensions": 512,
            "pooling": "last",
            "normalization": "l2",
            "instruction_policy": "v1",
            "text_format": 1,
            "runtime_build": "llama.cpp-b3560"
        },
        "backend": "native",
        "device": "cpu",
        "accelerated": false,
        "gpu_lane_verified": false,
        "corpus_commit": "HEAD",
        "canonical_revision": 42,
        "vector_revision": 42,
        "eligible_symbols": 100,
        "embedded_symbols": 100,
        "timestamp": "2026-09-09T02:00:00Z",
        "qualified": true,
        "unverified_lanes": ["metal", "vulkan", "cuda"]
    });
    validate_native_qualification(&val).expect("valid qwen record")
}

#[test]
fn test_qualification_reconciles_happy_path() {
    let record = sample_bge_record();
    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key: encoder_key.clone(),
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    let active_facts = ActiveProviderFacts {
        backend: "native",
        encoder_identity: Some(&record.encoder_identity),
        device: Some("cpu"),
        accelerated: Some(false),
        running_executable_sha: Some(&record.executable_sha256),
    };

    let res = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_facts,
    );
    assert!(res.is_ok(), "expected Ok, got {res:?}");
}

#[test]
fn test_qualification_rejects_backend_mismatch() {
    let record = sample_bge_record();
    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key,
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    let active_facts = ActiveProviderFacts {
        backend: "sidecar",
        encoder_identity: Some(&record.encoder_identity),
        device: Some("cpu"),
        accelerated: Some(false),
        running_executable_sha: Some(&record.executable_sha256),
    };

    let err = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_facts,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            WorkspaceQualificationMismatch::BackendMismatch {
                ref record_backend,
                ref active_backend,
            } if record_backend == "native" && active_backend == "sidecar"
        ),
        "expected BackendMismatch, got {err:?}"
    );
}

#[test]
fn test_qualification_rejects_active_encoder_mismatch() {
    let record = sample_bge_record();
    let qwen_rec = sample_qwen_record();
    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key,
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    let active_facts = ActiveProviderFacts {
        backend: "native",
        encoder_identity: Some(&qwen_rec.encoder_identity),
        device: Some("cpu"),
        accelerated: Some(false),
        running_executable_sha: Some(&record.executable_sha256),
    };

    let err = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_facts,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            WorkspaceQualificationMismatch::ActiveEncoderMismatch {
                ref record_model,
                ref active_model,
            } if record_model == "bge-small-en-v1.5-f32" && active_model == "qwen3-0.6b-f16"
        ),
        "expected ActiveEncoderMismatch, got {err:?}"
    );
}

#[test]
fn test_qualification_rejects_device_mismatch() {
    let record = sample_bge_record();
    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key,
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    let active_facts = ActiveProviderFacts {
        backend: "native",
        encoder_identity: Some(&record.encoder_identity),
        device: Some("Apple M1"),
        accelerated: Some(false),
        running_executable_sha: Some(&record.executable_sha256),
    };

    let err = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_facts,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            WorkspaceQualificationMismatch::DeviceMismatch {
                ref record_device,
                ref active_device,
            } if record_device == "cpu" && active_device == "Apple M1"
        ),
        "expected DeviceMismatch, got {err:?}"
    );
}

#[test]
fn test_qualification_rejects_acceleration_mismatch() {
    let record = sample_bge_record();
    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key,
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    let active_facts = ActiveProviderFacts {
        backend: "native",
        encoder_identity: Some(&record.encoder_identity),
        device: Some("cpu"),
        accelerated: Some(true),
        running_executable_sha: Some(&record.executable_sha256),
    };

    let err = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_facts,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            WorkspaceQualificationMismatch::AccelerationMismatch {
                record_accel: false,
                active_accel: true,
            }
        ),
        "expected AccelerationMismatch, got {err:?}"
    );
}

#[test]
fn test_qualification_rejects_missing_running_sidecar_sha() {
    let record = sample_bge_record();
    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key,
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    let active_facts = ActiveProviderFacts {
        backend: "native",
        encoder_identity: Some(&record.encoder_identity),
        device: Some("cpu"),
        accelerated: Some(false),
        running_executable_sha: None,
    };

    let err = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_facts,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            WorkspaceQualificationMismatch::MissingRunningSidecarSha
        ),
        "expected MissingRunningSidecarSha, got {err:?}"
    );
}

#[test]
fn test_qualification_rejects_executable_sha_mismatch() {
    let record = sample_bge_record();
    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key,
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    let active_facts = ActiveProviderFacts {
        backend: "native",
        encoder_identity: Some(&record.encoder_identity),
        device: Some("cpu"),
        accelerated: Some(false),
        running_executable_sha: Some(
            "0000000000000000000000000000000000000000000000000000000000000000",
        ),
    };

    let err = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_facts,
    )
    .unwrap_err();

    assert!(
        matches!(
            err,
            WorkspaceQualificationMismatch::ExecutableShaMismatch { .. }
        ),
        "expected ExecutableShaMismatch, got {err:?}"
    );
}

// ============================================================================
// Empirical Challenge Tests: Live Runtime Provider Switching & Health Output
// ============================================================================

use crate::health::{
    CanonicalStoreHealth, ControlPlaneHealth, DaemonLifecycleState, DataPlaneHealth,
    EmbeddingRuntimeHealth, EmbeddingState, HealthLevel, IndexingHealth, RuntimePlaneHealth,
    SystemHealthSnapshot, SystemStatus, WatcherState,
};

fn dummy_snapshot(
    qual: Option<crate::request_engine::NativeQualificationRecord>,
) -> SystemHealthSnapshot {
    SystemHealthSnapshot {
        overall: HealthLevel::Ready,
        readiness: SystemStatus::FullyReady { symbol_count: 100 },
        control_plane: ControlPlaneHealth {
            level: HealthLevel::Ready,
            daemon_state: DaemonLifecycleState::Direct,
            primary_workspace_id: Some("test_ws".to_string()),
            watcher_state: WatcherState::Local,
            watcher_ref_count: None,
            watcher_grace_active: false,
            detail: "Direct stdio session".to_string(),
        },
        data_plane: DataPlaneHealth {
            level: HealthLevel::Ready,
            canonical_store: CanonicalStoreHealth {
                level: HealthLevel::Ready,
                symbol_count: 100,
                file_count: 10,
                relationship_count: 50,
                embedding_count: 100,
                db_size_mb: 1.5,
                languages: vec!["rust".to_string()],
                detail: String::new(),
            },
            projections: Vec::new(),
            indexing: IndexingHealth {
                level: HealthLevel::Ready,
                active_operation: None,
                stage: None,
                catchup_active: false,
                watcher_paused: false,
                watcher_rescan_pending: false,
                dirty_projection_count: 0,
                repair_needed: false,
                repair_issue_count: 0,
                repair_reasons: Vec::new(),
                detail: String::new(),
            },
        },
        runtime_plane: RuntimePlaneHealth {
            level: HealthLevel::Ready,
            embeddings: EmbeddingRuntimeHealth {
                level: HealthLevel::Ready,
                state: EmbeddingState::Initialized,
                runtime: "native".to_string(),
                requested_backend: "native".to_string(),
                backend: "native".to_string(),
                device: "cpu".to_string(),
                accelerated: false,
                detail: String::new(),
                query_fallback: "off".to_string(),
            },
        },
        qualification: qual,
    }
}

#[test]
fn test_challenge_switching_provider_model_disqualifies_record_in_health_output() {
    // Database has a previously qualified BGE record
    let mut record = sample_bge_record();
    assert!(record.qualified, "baseline record must be qualified");
    assert!(record.disqualification_reason.is_none());

    let qwen_rec = sample_qwen_record();
    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key,
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    // Active provider is switched to running Qwen model
    let active_facts = ActiveProviderFacts {
        backend: "native",
        encoder_identity: Some(&qwen_rec.encoder_identity),
        device: Some("cpu"),
        accelerated: Some(false),
        running_executable_sha: Some(&record.executable_sha256),
    };

    // Emulate check_health integration logic in src/health/checker.rs:142-156
    match validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_facts,
    ) {
        Ok(()) => {
            record.qualified = true;
            record.disqualification_reason = None;
        }
        Err(mismatch) => {
            record.qualified = false;
            record.disqualification_reason = Some(mismatch.to_string());
        }
    }

    // 1. Verify that check_health flags record.qualified = false
    assert!(!record.qualified, "record must be flagged as disqualified");

    // 2. Verify disqualification reason is populated and accurate
    let reason = record
        .disqualification_reason
        .as_ref()
        .expect("disqualification reason must be present");
    assert!(
        reason.contains("active encoder mismatch"),
        "expected active encoder mismatch in reason, got: {reason}"
    );
    assert!(reason.contains("bge-small-en-v1.5-f32"));
    assert!(reason.contains("qwen3-0.6b-f16"));

    // 3. Verify health output structured serialization
    let snapshot = dummy_snapshot(Some(record));
    let structured = serde_json::to_value(&snapshot).expect("serialize snapshot");
    assert_eq!(
        structured["qualification"]["qualified"], false,
        "structured output must show qualified=false"
    );
    assert_eq!(
        structured["qualification"]["disqualification_reason"],
        "active encoder mismatch: record has 'bge-small-en-v1.5-f32', active provider has 'qwen3-0.6b-f16'"
    );
}

#[test]
fn test_challenge_switching_provider_backend_disqualifies_record_in_health_output() {
    // Database has a previously qualified native BGE record
    let mut record = sample_bge_record();
    assert!(record.qualified);
    assert!(record.disqualification_reason.is_none());

    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key,
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    // Active provider is switched to Python / non-native backend
    let active_facts = ActiveProviderFacts {
        backend: "python",
        encoder_identity: Some(&record.encoder_identity),
        device: Some("cpu"),
        accelerated: Some(false),
        running_executable_sha: Some(&record.executable_sha256),
    };

    // Emulate check_health integration logic in src/health/checker.rs:142-156
    match validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_facts,
    ) {
        Ok(()) => {
            record.qualified = true;
            record.disqualification_reason = None;
        }
        Err(mismatch) => {
            record.qualified = false;
            record.disqualification_reason = Some(mismatch.to_string());
        }
    }

    // 1. Verify record.qualified = false
    assert!(!record.qualified);

    // 2. Verify disqualification reason
    let reason = record
        .disqualification_reason
        .as_ref()
        .expect("reason must be present");
    assert!(reason.contains("backend mismatch"), "got: {reason}");
    assert!(reason.contains("python"));

    // 3. Verify health output structured serialization
    let snapshot = dummy_snapshot(Some(record));
    let structured = serde_json::to_value(&snapshot).expect("serialize snapshot");
    assert_eq!(structured["qualification"]["qualified"], false);
    assert_eq!(
        structured["qualification"]["disqualification_reason"],
        "backend mismatch: record has 'native', active runtime has 'python'"
    );
}

#[test]
fn test_challenge_render_report_text_health_output() {
    let mut record = sample_bge_record();
    record.qualified = false;
    record.disqualification_reason =
        Some("backend mismatch: record has 'native', active runtime has 'python'".to_string());

    let snapshot = dummy_snapshot(Some(record));
    let report = snapshot.render_report(true);

    // Verify text output flags qualified: false
    assert!(
        report.contains("Qualified: false"),
        "text report must state Qualified: false"
    );

    // Check whether render_report includes Disqualification Reason
    // Note: render_report in src/health/report.rs currently does not render disqualification_reason
    let has_reason_in_text = report.contains("Disqualification Reason");
    assert!(
        !has_reason_in_text,
        "empirical finding: render_report omits Disqualification Reason from text output"
    );
}

#[test]
fn test_challenge_active_encoder_none_fails_validation() {
    let record = sample_bge_record();
    let encoder_key = record.encoder_identity.storage_key().unwrap();
    let ready_gen = EmbeddingGeneration {
        id: 1,
        encoder_key,
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 1000,
        updated_at: 1000,
    };

    // Case 1: Active provider has no encoder identity -> must fail
    let active_no_id = ActiveProviderFacts {
        backend: "native",
        encoder_identity: None,
        device: Some("cpu"),
        accelerated: Some(false),
        running_executable_sha: Some(&record.executable_sha256),
    };

    let res = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_no_id,
    );
    assert!(
        res.is_err(),
        "missing encoder identity must fail qualification"
    );
    let err_str = res.unwrap_err().to_string();
    assert!(
        err_str.contains("missing live evidence"),
        "error must mention missing live evidence, got: {err_str}"
    );

    // Case 2: Active provider has no device -> must fail
    let active_no_device = ActiveProviderFacts {
        backend: "native",
        encoder_identity: Some(&record.encoder_identity),
        device: None,
        accelerated: Some(false),
        running_executable_sha: Some(&record.executable_sha256),
    };
    let res_dev = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_no_device,
    );
    assert!(res_dev.is_err(), "missing device must fail qualification");

    // Case 3: Active provider has no acceleration -> must fail
    let active_no_accel = ActiveProviderFacts {
        backend: "native",
        encoder_identity: Some(&record.encoder_identity),
        device: Some("cpu"),
        accelerated: None,
        running_executable_sha: Some(&record.executable_sha256),
    };
    let res_acc = validate_qualification_against_running_runtime(
        &record,
        record.canonical_revision,
        Some(&ready_gen),
        &active_no_accel,
    );
    assert!(
        res_acc.is_err(),
        "missing acceleration must fail qualification"
    );
}
