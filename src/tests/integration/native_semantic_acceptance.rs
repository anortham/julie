//! Milestone N5: Acceptance and CPU Qualification tests.
//!
//! Validates machine-readable native semantic qualification records, rejection of
//! unverified or invalid artifacts, CPU qualification baseline for BGE and Qwen models,
//! preservation of Python provider defaults, and workspace health integration.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use crate::request_engine::SemanticMode;
use crate::request_engine::semantic::semantic_mode_needs_provider;
use crate::request_engine::semantic_qualification::{
    QUALIFICATION_SCHEMA_V1, QualificationValidationError, WorkspaceQualificationMismatch,
    load_workspace_qualification, validate_native_qualification,
    validate_qualification_against_workspace, workspace_qualification_path,
};
use crate::tests::helpers::workspace::make_isolated_workspace_root;
use julie_core::database::{EmbeddingGeneration, EmbeddingGenerationStatus};
use julie_core::embeddings_contract::EmbeddingBackend;
use julie_pipeline::embeddings::factory::{
    BackendResolverCapabilities, parse_provider_preference, resolve_backend_preference,
};

fn sample_valid_bge_record_value() -> Value {
    json!({
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
        "facts_revision": 42,
        "vector_revision": 42,
        "eligible_symbols": 100,
        "embedded_symbols": 100,
        "timestamp": "2026-09-09T02:00:00Z",
        "qualified": true,
        "unverified_lanes": ["metal", "vulkan", "cuda"]
    })
}

fn sample_valid_qwen_record_value() -> Value {
    let mut v = sample_valid_bge_record_value();
    v["encoder_identity"] = json!({
        "schema": 1,
        "model_id": "qwen3-0.6b-f16",
        "weights_sha256": "421a27e58d165478cc7acb984a688c2aa41404968b0203e7cd743ece44c54340",
        "dimensions": 512,
        "pooling": "last",
        "normalization": "l2",
        "instruction_policy": "v1",
        "text_format": 1,
        "runtime_build": "llama.cpp-b3560"
    });
    v
}

// ============================================================================
// Section A: Pure Unit Anchor & Rejection Suite
// ============================================================================

#[test]
fn native_semantic_qualification_rejects_unverified_artifact() {
    let value = json!({
        "schema": QUALIFICATION_SCHEMA_V1
    });
    let result = validate_native_qualification(&value);
    assert!(result.is_err(), "unverified artifact must be rejected");
}

#[test]
fn test_rejection_missing_or_bad_schema() {
    let mut v = sample_valid_bge_record_value();
    v["schema"] = json!("unknown-schema-v0");
    let err = validate_native_qualification(&v).expect_err("should reject bad schema");
    assert!(matches!(
        err,
        QualificationValidationError::InvalidSchema { .. }
    ));

    let mut v_empty = sample_valid_bge_record_value();
    v_empty["schema"] = json!("");
    let err_empty =
        validate_native_qualification(&v_empty).expect_err("should reject empty schema");
    assert!(matches!(
        err_empty,
        QualificationValidationError::InvalidSchema { .. }
    ));
}

#[test]
fn test_rejection_unqualified_record() {
    let mut v = sample_valid_bge_record_value();
    v["qualified"] = json!(false);
    let err = validate_native_qualification(&v).expect_err("should reject qualified: false");
    assert_eq!(err, QualificationValidationError::UnqualifiedRecord);
}

#[test]
fn test_rejection_bad_source_shas() {
    let mut v1 = sample_valid_bge_record_value();
    v1["julie_source_sha"] = json!("short_sha");
    let err1 =
        validate_native_qualification(&v1).expect_err("should reject short julie_source_sha");
    assert!(matches!(
        err1,
        QualificationValidationError::InvalidJulieSourceSha(_)
    ));

    let mut v2 = sample_valid_bge_record_value();
    v2["sidecar_source_sha"] = json!("zzzz5518d49c88e9d0d696f408f0c6e8fd165751");
    let err2 =
        validate_native_qualification(&v2).expect_err("should reject non-hex sidecar_source_sha");
    assert!(matches!(
        err2,
        QualificationValidationError::InvalidSidecarSourceSha(_)
    ));
}

#[test]
fn test_rejection_bad_executable_hash() {
    let mut v = sample_valid_bge_record_value();
    // Uppercase hex must be rejected
    v["executable_sha256"] =
        json!("C27B95FB1E975884C11C964814B019618B959B894AB19437FF641ECA648AC51D");
    let err =
        validate_native_qualification(&v).expect_err("should reject uppercase executable hash");
    assert!(matches!(
        err,
        QualificationValidationError::InvalidExecutableSha256(_)
    ));

    let mut v_short = sample_valid_bge_record_value();
    v_short["executable_sha256"] = json!("c27b95fb");
    let err_short =
        validate_native_qualification(&v_short).expect_err("should reject short executable hash");
    assert!(matches!(
        err_short,
        QualificationValidationError::InvalidExecutableSha256(_)
    ));
}

#[test]
fn test_rejection_invalid_encoder_identity() {
    let mut v = sample_valid_bge_record_value();
    v["encoder_identity"]["schema"] = json!(0);
    let err = validate_native_qualification(&v).expect_err("should reject schema 0");
    assert!(matches!(
        err,
        QualificationValidationError::InvalidEncoderIdentity(_)
    ));

    let mut v2 = sample_valid_bge_record_value();
    v2["encoder_identity"]["model_id"] = json!("");
    let err2 = validate_native_qualification(&v2).expect_err("should reject empty model_id");
    assert!(matches!(
        err2,
        QualificationValidationError::InvalidEncoderIdentity(_)
    ));
}

#[test]
fn test_rejection_mismatched_model_dimensions() {
    // BGE must have 384 dimensions
    let mut v_bge = sample_valid_bge_record_value();
    v_bge["encoder_identity"]["dimensions"] = json!(512);
    let err_bge =
        validate_native_qualification(&v_bge).expect_err("should reject 512 dimensions for BGE");
    assert!(matches!(
        err_bge,
        QualificationValidationError::InvalidEncoderIdentity(_)
    ));

    // Qwen must have 512 served dimensions
    let mut v_qwen = sample_valid_qwen_record_value();
    v_qwen["encoder_identity"]["dimensions"] = json!(384);
    let err_qwen =
        validate_native_qualification(&v_qwen).expect_err("should reject 384 dimensions for Qwen");
    assert!(matches!(
        err_qwen,
        QualificationValidationError::InvalidEncoderIdentity(_)
    ));
}

#[test]
fn test_rejection_mismatched_pooling() {
    // BGE expects cls pooling
    let mut v_bge = sample_valid_bge_record_value();
    v_bge["encoder_identity"]["pooling"] = json!("last");
    let err_bge =
        validate_native_qualification(&v_bge).expect_err("should reject 'last' pooling for BGE");
    assert!(matches!(
        err_bge,
        QualificationValidationError::InvalidEncoderIdentity(_)
    ));

    // Qwen expects last pooling
    let mut v_qwen = sample_valid_qwen_record_value();
    v_qwen["encoder_identity"]["pooling"] = json!("cls");
    let err_qwen =
        validate_native_qualification(&v_qwen).expect_err("should reject 'cls' pooling for Qwen");
    assert!(matches!(
        err_qwen,
        QualificationValidationError::InvalidEncoderIdentity(_)
    ));
}

#[test]
fn test_rejection_python_and_sidecar_backend() {
    let mut v_py = sample_valid_bge_record_value();
    v_py["backend"] = json!("python");
    let err_py =
        validate_native_qualification(&v_py).expect_err("should reject python backend in native");
    assert_eq!(err_py, QualificationValidationError::PythonBackendForbidden);

    let mut v_sc = sample_valid_bge_record_value();
    v_sc["backend"] = json!("sidecar");
    let err_sc =
        validate_native_qualification(&v_sc).expect_err("should reject sidecar backend in native");
    assert_eq!(err_sc, QualificationValidationError::PythonBackendForbidden);
}

#[test]
fn test_qualification_rejects_unknown_backend() {
    let mut v = sample_valid_bge_record_value();
    v["backend"] = json!("unsupported_accelerator");
    let err = validate_native_qualification(&v).expect_err("should reject unknown backend");
    assert_eq!(
        err,
        QualificationValidationError::UnknownBackend("unsupported_accelerator".to_string())
    );
}

#[test]
fn test_rejection_empty_backend_and_device() {
    let mut v_be = sample_valid_bge_record_value();
    v_be["backend"] = json!("  ");
    let err_be = validate_native_qualification(&v_be).expect_err("should reject empty backend");
    assert_eq!(err_be, QualificationValidationError::EmptyBackend);

    let mut v_dev = sample_valid_bge_record_value();
    v_dev["device"] = json!("");
    let err_dev = validate_native_qualification(&v_dev).expect_err("should reject empty device");
    assert_eq!(err_dev, QualificationValidationError::EmptyDevice);
}

#[test]
fn test_rejection_inconsistent_acceleration() {
    // CPU device cannot claim accelerated: true
    let mut v1 = sample_valid_bge_record_value();
    v1["device"] = json!("cpu");
    v1["accelerated"] = json!(true);
    let err1 = validate_native_qualification(&v1)
        .expect_err("should reject accelerated: true on CPU device");
    assert!(matches!(
        err1,
        QualificationValidationError::InconsistentAcceleration { .. }
    ));

    // CPU device cannot claim gpu_lane_verified: true
    let mut v2 = sample_valid_bge_record_value();
    v2["device"] = json!("cpu");
    v2["accelerated"] = json!(false);
    v2["gpu_lane_verified"] = json!(true);
    let err2 = validate_native_qualification(&v2)
        .expect_err("should reject gpu_lane_verified: true on CPU device");
    assert!(matches!(
        err2,
        QualificationValidationError::InconsistentAcceleration { .. }
    ));

    // gpu_lane_verified cannot be true if accelerated is false
    let mut v3 = sample_valid_bge_record_value();
    v3["device"] = json!("NVIDIA RTX 4090");
    v3["accelerated"] = json!(false);
    v3["gpu_lane_verified"] = json!(true);
    let err3 = validate_native_qualification(&v3)
        .expect_err("should reject gpu_lane_verified: true without accelerated: true");
    assert!(matches!(
        err3,
        QualificationValidationError::InconsistentAcceleration { .. }
    ));
}

#[test]
fn test_rejection_empty_corpus_commit() {
    let mut v = sample_valid_bge_record_value();
    v["corpus_commit"] = json!("   ");
    let err = validate_native_qualification(&v).expect_err("should reject empty corpus_commit");
    assert_eq!(err, QualificationValidationError::EmptyCorpusCommit);
}

#[test]
fn test_rejection_revision_lag() {
    let mut v = sample_valid_bge_record_value();
    v["facts_revision"] = json!(42);
    v["vector_revision"] = json!(41);
    let err = validate_native_qualification(&v).expect_err("should reject revision lag");
    assert_eq!(
        err,
        QualificationValidationError::RevisionMismatch {
            canonical: 42,
            vector: 41
        }
    );
}

#[test]
fn test_qualification_rejects_negative_revisions() {
    let mut v = sample_valid_bge_record_value();
    v["facts_revision"] = json!(-1);
    v["vector_revision"] = json!(-1);
    let err = validate_native_qualification(&v).expect_err("should reject negative revisions");
    assert_eq!(
        err,
        QualificationValidationError::NegativeRevision {
            canonical: -1,
            vector: -1
        }
    );
}

#[test]
fn test_rejection_incomplete_vector_coverage() {
    let mut v = sample_valid_bge_record_value();
    v["eligible_symbols"] = json!(100);
    v["embedded_symbols"] = json!(99);
    let err = validate_native_qualification(&v).expect_err("should reject incomplete coverage");
    assert_eq!(
        err,
        QualificationValidationError::IncompleteCoverage {
            eligible: 100,
            embedded: 99
        }
    );
}

#[test]
fn test_rejection_invalid_timestamp() {
    let mut v = sample_valid_bge_record_value();
    v["timestamp"] = json!("not-a-valid-rfc3339-timestamp");
    let err = validate_native_qualification(&v).expect_err("should reject invalid timestamp");
    assert!(matches!(
        err,
        QualificationValidationError::InvalidTimestamp(_)
    ));
}

// ============================================================================
// Section B: CPU Qualification Baseline Records
// ============================================================================

#[test]
fn test_accepts_valid_cpu_qualification_baseline_bge() {
    let v = sample_valid_bge_record_value();
    let record = validate_native_qualification(&v).expect("valid BGE CPU record must pass");
    assert!(record.qualified);
    assert_eq!(record.schema, QUALIFICATION_SCHEMA_V1);
    assert_eq!(record.encoder_identity.model_id, "bge-small-en-v1.5-f32");
    assert_eq!(record.encoder_identity.dimensions, 384);
    assert_eq!(record.encoder_identity.pooling, "cls");
    assert_eq!(record.device, "cpu");
    assert!(!record.accelerated);
    assert!(!record.gpu_lane_verified);
    assert_eq!(record.facts_revision, record.vector_revision);
    assert_eq!(record.embedded_symbols, record.eligible_symbols);
    assert_eq!(
        record.unverified_lanes,
        Some(vec![
            "metal".to_string(),
            "vulkan".to_string(),
            "cuda".to_string()
        ])
    );
}

#[test]
fn test_accepts_valid_cpu_qualification_baseline_qwen() {
    let v = sample_valid_qwen_record_value();
    let record = validate_native_qualification(&v).expect("valid Qwen CPU record must pass");
    assert!(record.qualified);
    assert_eq!(record.schema, QUALIFICATION_SCHEMA_V1);
    assert_eq!(record.encoder_identity.model_id, "qwen3-0.6b-f16");
    assert_eq!(record.encoder_identity.dimensions, 512);
    assert_eq!(record.encoder_identity.pooling, "last");
    assert_eq!(record.device, "cpu");
    assert!(!record.accelerated);
    assert!(!record.gpu_lane_verified);
    assert_eq!(record.facts_revision, record.vector_revision);
    assert_eq!(record.embedded_symbols, record.eligible_symbols);
}

#[test]
fn test_accepts_record_with_resolved_backend_and_qualified_at_aliases() {
    // Verifies alias compatibility with docs/SEMANTIC_PROVIDERS.md Section 8
    let v = json!({
        "schema": "julie-native-qualification-v1",
        "julie_source_sha": "0432158c88e9d0d696f408f0c6e8fd1657513b0a",
        "sidecar_source_sha": "9ed082ba511aa8b10c9e7b47110c3a4dd1e98d59",
        "executable_sha256": "3d7267551e975884c11c964814b019618b959b894ab19437ff641eca648ac51d",
        "encoder_identity": {
            "schema": 1,
            "model_id": "bge-small-en-v1.5-f32",
            "weights_sha256": "bf40c42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65",
            "dimensions": 384,
            "pooling": "cls",
            "normalization": "l2",
            "instruction_policy": "v1",
            "text_format": 3,
            "runtime_build": "llama.cpp-b3560"
        },
        "device": "cpu",
        "resolved_backend": "cpu",
        "accelerated": false,
        "corpus_commit": "HEAD",
        "facts_revision": 142,
        "vector_revision": 142,
        "eligible_symbols": 8420,
        "embedded_symbols": 8420,
        "qualified": true,
        "qualified_at": "2026-09-09T02:00:00Z"
    });
    let record = validate_native_qualification(&v).expect("doc format must parse and validate");
    assert!(record.qualified);
    assert_eq!(record.backend, "cpu");
    assert_eq!(record.timestamp, "2026-09-09T02:00:00Z");
}

// ============================================================================
// Section C: Provider Selection
// ============================================================================

#[test]
fn test_auto_resolves_to_native_when_sidecar_binary_found() {
    let backend = parse_provider_preference("auto").expect("auto must parse");
    assert_eq!(backend, EmbeddingBackend::Auto);

    let caps = BackendResolverCapabilities {
        native_available: true,
        ..BackendResolverCapabilities::current()
    };
    let resolved = resolve_backend_preference(backend, &caps).expect("must resolve");
    assert_eq!(resolved, EmbeddingBackend::Native);
}

#[test]
fn test_explicit_provider_selection_rejects_sidecar_and_keeps_native() {
    assert!(parse_provider_preference("sidecar").is_err());

    let native_pref = parse_provider_preference("native").expect("native must parse");
    assert_eq!(native_pref, EmbeddingBackend::Native);
}

#[test]
fn test_off_mode_gates_provider_acquisition() {
    assert!(!semantic_mode_needs_provider(SemanticMode::Off));
    assert!(semantic_mode_needs_provider(SemanticMode::Auto));
    assert!(semantic_mode_needs_provider(SemanticMode::Required));
}

// ============================================================================
// Section D: Workspace Health Structured Output Integration
// ============================================================================

#[test]
fn test_workspace_qualification_load_and_health_integration() {
    let temp = tempfile::tempdir().expect("create tempdir");
    let root_buf = make_isolated_workspace_root(temp.path(), "qual_ws");
    let root = root_buf.as_path();

    // 1. Initially no qualification file exists
    assert!(load_workspace_qualification(root).is_none());

    // 2. Write valid qualification record to <workspace>/.julie/qualification.json
    let qual_dir = root.join(".julie");
    fs::create_dir_all(&qual_dir).expect("create .julie dir");
    let qual_file = workspace_qualification_path(root);
    let valid_json = serde_json::to_string_pretty(&sample_valid_bge_record_value())
        .expect("serialize valid record");
    fs::write(&qual_file, valid_json).expect("write qualification.json");

    // 3. load_workspace_qualification must succeed
    let loaded =
        load_workspace_qualification(root).expect("must load and validate qualification from disk");
    assert!(loaded.qualified);
    assert_eq!(loaded.encoder_identity.model_id, "bge-small-en-v1.5-f32");

    // 4. Overwrite with invalid file -> load_workspace_qualification must return None
    fs::write(&qual_file, "{\"schema\": \"invalid\"}").expect("overwrite with invalid");
    assert!(load_workspace_qualification(root).is_none());
}

#[test]
fn test_workspace_qualification_rejects_stale_or_diverged_workspace_state() {
    let record_val = sample_valid_bge_record_value();
    let record = validate_native_qualification(&record_val).expect("valid baseline");
    let encoder_key = record
        .encoder_identity
        .storage_key()
        .expect("encoder storage key");

    let matching_gen = EmbeddingGeneration {
        id: 1,
        encoder_key: encoder_key.clone(),
        source_revision: record.vector_revision,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: record.eligible_symbols,
        embedded_symbols: record.embedded_symbols,
        created_at: 1000,
        updated_at: 1000,
    };

    // 1. Matching DB state passes qualification
    assert!(
        validate_qualification_against_workspace(
            &record,
            record.facts_revision,
            Some(&matching_gen),
            Some(&record.executable_sha256),
        )
        .is_ok()
    );

    // 2. Bumping DB revision causes CanonicalRevisionMismatch
    let bumped_rev = record.facts_revision + 1;
    let rev_err = validate_qualification_against_workspace(
        &record,
        bumped_rev,
        Some(&matching_gen),
        Some(&record.executable_sha256),
    )
    .unwrap_err();
    assert!(matches!(
        rev_err,
        WorkspaceQualificationMismatch::CanonicalRevisionMismatch { .. }
    ));

    // 3. Symbol count change causes CoverageMismatch
    let diverged_gen = EmbeddingGeneration {
        embedded_symbols: record.embedded_symbols - 1,
        ..matching_gen.clone()
    };
    let cov_err = validate_qualification_against_workspace(
        &record,
        record.facts_revision,
        Some(&diverged_gen),
        Some(&record.executable_sha256),
    )
    .unwrap_err();
    assert!(matches!(
        cov_err,
        WorkspaceQualificationMismatch::CoverageMismatch { .. }
    ));

    // 4. Switching model key causes EncoderKeyMismatch
    let other_gen = EmbeddingGeneration {
        encoder_key: "different_model_storage_key_1234567890abcdef".to_string(),
        ..matching_gen.clone()
    };
    let model_err = validate_qualification_against_workspace(
        &record,
        record.facts_revision,
        Some(&other_gen),
        Some(&record.executable_sha256),
    )
    .unwrap_err();
    assert!(matches!(
        model_err,
        WorkspaceQualificationMismatch::EncoderKeyMismatch { .. }
    ));

    // 5. No ready generation causes NoReadyGeneration
    let no_gen_err = validate_qualification_against_workspace(
        &record,
        record.facts_revision,
        None,
        Some(&record.executable_sha256),
    )
    .unwrap_err();
    assert!(matches!(
        no_gen_err,
        WorkspaceQualificationMismatch::NoReadyGeneration
    ));
}

#[test]
#[ignore = "requires julie-semantic-sidecar binary and cached GGUF models"]
fn test_real_sidecar_and_models_present_for_cpu_qualification() {
    use julie_core::embeddings_contract::{EmbeddingProvider, EmbeddingRequestBudget};
    use julie_pipeline::embeddings::{EmbeddingConfig, NativeEmbeddingProvider};
    use std::time::Duration;

    let sidecar_path = Path::new(
        "/home/murphy/source/julie-semantic-sidecar/target/release/julie-semantic-sidecar",
    );
    assert!(
        sidecar_path.is_file(),
        "sidecar binary missing at {:?}",
        sidecar_path
    );

    let (found_path, hash) =
        julie_pipeline::embeddings::native::launch::find_and_hash_sidecar_binary(Some(
            sidecar_path,
        ))
        .expect("sidecar binary must hash cleanly");
    assert_eq!(found_path, sidecar_path);
    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(30));

    // 1. Real BGE inference (384 finite floats)
    let bge_config = EmbeddingConfig {
        provider: "native".to_string(),
        native_program: Some(sidecar_path.to_path_buf()),
        native_model: Some("bge-small-en-v1.5-f32".to_string()),
        ..Default::default()
    };
    let bge_provider =
        NativeEmbeddingProvider::try_new(&bge_config).expect("initialize BGE provider");
    let bge_vec = bge_provider
        .embed_query("fn authenticate_user() -> bool", &budget)
        .expect("embed BGE query");
    assert_eq!(bge_vec.len(), 384, "BGE must return 384 dimensions");
    assert!(
        bge_vec.iter().all(|f| f.is_finite()),
        "all BGE floats must be finite"
    );

    // 2. Real Qwen inference (512 dimensions)
    let qwen_config = EmbeddingConfig {
        provider: "native".to_string(),
        native_program: Some(sidecar_path.to_path_buf()),
        native_model: Some("qwen3-0.6b-f16".to_string()),
        ..Default::default()
    };
    let qwen_provider =
        NativeEmbeddingProvider::try_new(&qwen_config).expect("initialize Qwen provider");
    let qwen_vec = qwen_provider
        .embed_query("fn authenticate_user() -> bool", &budget)
        .expect("embed Qwen query");
    assert_eq!(qwen_vec.len(), 512, "Qwen must return 512 dimensions");
    assert!(
        qwen_vec.iter().all(|f| f.is_finite()),
        "all Qwen floats must be finite"
    );
}

// ============================================================================
// Section E: Challenger 1 Adversarial Stress Suite
// ============================================================================

#[test]
fn test_adversarial_corrupted_hashes() {
    let base = sample_valid_bge_record_value();

    for bad in [
        "5518d49c88e9d0d696f408f0c6e8fd1657513b0",
        "5518d49c88e9d0d696f408f0c6e8fd1657513b0aa",
        "g518d49c88e9d0d696f408f0c6e8fd1657513b0a",
        "5518d49c88e9d0d696f408f0c6e8fd1657513b0\n",
    ] {
        let mut v = base.clone();
        v["julie_source_sha"] = json!(bad);
        assert!(matches!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::InvalidJulieSourceSha(_)
        ));
    }

    for bad in [
        "9ed082ba511aa8b10c9e7b47110c3a4dd1e98d5",
        "9ed082ba511aa8b10c9e7b47110c3a4dd1e98d599",
    ] {
        let mut v = base.clone();
        v["sidecar_source_sha"] = json!(bad);
        assert!(matches!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::InvalidSidecarSourceSha(_)
        ));
    }

    for bad in [
        "c27b95fb1e975884c11c964814b019618b959b894ab19437ff641eca648ac51",
        "c27b95fb1e975884c11c964814b019618b959b894ab19437ff641eca648ac51dd",
        "C27b95fb1e975884c11c964814b019618b959b894ab19437ff641eca648ac51d",
    ] {
        let mut v = base.clone();
        v["executable_sha256"] = json!(bad);
        assert!(matches!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::InvalidExecutableSha256(_)
        ));
    }

    // 10. weights_sha256: invalid length and non-hex in encoder_identity
    for bad in [
        "bf40c42ad7d89382e9ba7376d5c4b73f",
        "zzzzc42ad7d89382e9ba7376d5c4b73f6b556cb541fab37aaa1da9c320149b65",
    ] {
        let mut v = base.clone();
        v["encoder_identity"]["weights_sha256"] = json!(bad);
        assert!(matches!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::InvalidEncoderIdentity(_)
        ));
    }
}

#[test]
fn test_adversarial_missing_fields() {
    let base = sample_valid_bge_record_value();
    let required_fields = [
        "schema",
        "julie_source_sha",
        "sidecar_source_sha",
        "executable_sha256",
        "encoder_identity",
        "backend",
        "device",
        "accelerated",
        "corpus_commit",
        "facts_revision",
        "vector_revision",
        "eligible_symbols",
        "embedded_symbols",
        "timestamp",
    ];

    for field in &required_fields {
        let mut v = base.clone();
        v.as_object_mut().unwrap().remove(*field);
        assert!(
            validate_native_qualification(&v).is_err(),
            "Record missing field '{}' must be rejected",
            field
        );
    }

    let required_identity_fields = [
        "schema",
        "model_id",
        "weights_sha256",
        "dimensions",
        "pooling",
        "normalization",
        "instruction_policy",
        "text_format",
        "runtime_build",
    ];

    for field in &required_identity_fields {
        let mut v = base.clone();
        v["encoder_identity"]
            .as_object_mut()
            .unwrap()
            .remove(*field);
        assert!(
            validate_native_qualification(&v).is_err(),
            "Record with encoder_identity missing '{}' must be rejected",
            field
        );
    }
}

#[test]
fn test_adversarial_schema_violations() {
    let base = sample_valid_bge_record_value();

    for bad_schema in [
        json!(1),
        json!(true),
        json!(null),
        json!(["julie-native-qualification-v1"]),
        json!("julie-native-qualification-v0"),
        json!("julie-native-qualification-v2"),
        json!("julie-native-qualification-v1\n"),
        json!(" julie-native-qualification-v1 "),
        json!(""),
    ] {
        let mut v = base.clone();
        v["schema"] = bad_schema;
        let err = validate_native_qualification(&v).expect_err("Invalid schema must be rejected");
        assert!(matches!(
            err,
            QualificationValidationError::InvalidSchema { .. }
        ));
    }
}

#[test]
fn test_adversarial_pooling_and_dimensions() {
    let check = |v: &Value| {
        assert!(matches!(
            validate_native_qualification(v).unwrap_err(),
            QualificationValidationError::InvalidEncoderIdentity(_)
        ));
    };

    let base_bge = sample_valid_bge_record_value();
    for dims in [0, 1, 383, 385, 512, 768, 1024] {
        let mut v = base_bge.clone();
        v["encoder_identity"]["dimensions"] = json!(dims);
        check(&v);
    }
    for pooling in ["last", "mean", "cls\n", "CLS", "", "none", "token"] {
        let mut v = base_bge.clone();
        v["encoder_identity"]["pooling"] = json!(pooling);
        check(&v);
    }

    let base_qwen = sample_valid_qwen_record_value();
    for dims in [0, 1, 384, 511, 513, 768, 1024] {
        let mut v = base_qwen.clone();
        v["encoder_identity"]["dimensions"] = json!(dims);
        check(&v);
    }
    for pooling in ["cls", "mean", "last\n", "LAST", "", "none", "token"] {
        let mut v = base_qwen.clone();
        v["encoder_identity"]["pooling"] = json!(pooling);
        check(&v);
    }
    for norm in ["l3", "batch", "zscore", "", "none\n"] {
        let mut v = base_bge.clone();
        v["encoder_identity"]["normalization"] = json!(norm);
        check(&v);
    }
}

#[test]
fn test_adversarial_unverified_gpu_and_device_claims() {
    let base = sample_valid_bge_record_value();

    // 1. CPU device claiming accelerated or gpu_lane_verified
    for dev in ["cpu", "CPU", " Cpu ", "cpu\n"] {
        let mut v = base.clone();
        v["device"] = json!(dev);
        v["accelerated"] = json!(true);
        assert!(matches!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::InconsistentAcceleration { .. }
        ));

        let mut v2 = base.clone();
        v2["device"] = json!(dev);
        v2["accelerated"] = json!(false);
        v2["gpu_lane_verified"] = json!(true);
        assert!(matches!(
            validate_native_qualification(&v2).unwrap_err(),
            QualificationValidationError::InconsistentAcceleration { .. }
        ));
    }

    // 2. CPU backend claiming accelerated or gpu_lane_verified
    for backend in ["cpu", "CPU", " Cpu "] {
        let mut v = base.clone();
        v["backend"] = json!(backend);
        v["device"] = json!("accelerator");
        v["accelerated"] = json!(true);
        assert!(matches!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::InconsistentAcceleration { .. }
        ));

        let mut v2 = base.clone();
        v2["backend"] = json!(backend);
        v2["device"] = json!("accelerator");
        v2["accelerated"] = json!(false);
        v2["gpu_lane_verified"] = json!(true);
        assert!(matches!(
            validate_native_qualification(&v2).unwrap_err(),
            QualificationValidationError::InconsistentAcceleration { .. }
        ));
    }

    // 3. Device "unavailable" claiming accelerated or gpu_lane_verified
    for dev in ["unavailable", "Unavailable", " UNAVAILABLE "] {
        let mut v = base.clone();
        v["device"] = json!(dev);
        v["accelerated"] = json!(true);
        assert!(matches!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::InconsistentAcceleration { .. }
        ));

        let mut v2 = base.clone();
        v2["device"] = json!(dev);
        v2["gpu_lane_verified"] = json!(true);
        assert!(matches!(
            validate_native_qualification(&v2).unwrap_err(),
            QualificationValidationError::InconsistentAcceleration { .. }
        ));
    }

    // 4. GPU device claiming gpu_lane_verified: true without accelerated: true
    let mut v = base.clone();
    v["device"] = json!("Apple M1 Pro");
    v["accelerated"] = json!(false);
    v["gpu_lane_verified"] = json!(true);
    assert!(matches!(
        validate_native_qualification(&v).unwrap_err(),
        QualificationValidationError::InconsistentAcceleration { .. }
    ));

    // 5. Python/sidecar forbidden case-insensitively with whitespace
    for py in [
        "python",
        "PYTHON",
        " Python ",
        "sidecar",
        "SIDECAR",
        " Sidecar ",
    ] {
        let mut v = base.clone();
        v["backend"] = json!(py);
        assert_eq!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::PythonBackendForbidden
        );
    }

    // 6. Empty or whitespace-only corpus commit
    for commit in ["", "   ", "\t", "\n"] {
        let mut v = base.clone();
        v["corpus_commit"] = json!(commit);
        assert_eq!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::EmptyCorpusCommit
        );
    }

    // 7. Revision mismatches
    for (can, vec) in [(10, 9), (9, 10)] {
        let mut v = base.clone();
        v["facts_revision"] = json!(can);
        v["vector_revision"] = json!(vec);
        assert_eq!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::RevisionMismatch {
                canonical: can,
                vector: vec
            }
        );
    }

    // 9. Incomplete coverage
    let mut v = base.clone();
    v["eligible_symbols"] = json!(1000);
    v["embedded_symbols"] = json!(999);
    assert_eq!(
        validate_native_qualification(&v).unwrap_err(),
        QualificationValidationError::IncompleteCoverage {
            eligible: 1000,
            embedded: 999
        }
    );

    // 10. Invalid timestamps
    for ts in [
        "",
        "2026-09-09",
        "2026-09-09 02:00:00",
        "not-a-timestamp",
        "2026-02-31T00:00:00Z",
    ] {
        let mut v = base.clone();
        v["timestamp"] = json!(ts);
        assert!(matches!(
            validate_native_qualification(&v).unwrap_err(),
            QualificationValidationError::InvalidTimestamp(_)
        ));
    }
}
