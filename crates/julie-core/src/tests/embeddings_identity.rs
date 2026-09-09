use crate::embeddings_contract::EmbeddingRequestBudget;
use crate::embeddings_identity::EncoderIdentity;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn fixture_identity() -> EncoderIdentity {
    EncoderIdentity {
        schema: 1,
        model_id: "bge-small-en-v1.5-f32".into(),
        weights_sha256: "a".repeat(64),
        dimensions: 384,
        pooling: "cls".into(),
        normalization: "l2".into(),
        instruction_policy: "v1".into(),
        text_format: 1,
        runtime_build: "fixture-runtime".into(),
    }
}

#[test]
fn encoder_identity_changes_when_instruction_policy_changes() {
    let first = EncoderIdentity {
        schema: 1,
        model_id: "bge-small-en-v1.5-f32".into(),
        weights_sha256: "a".repeat(64),
        dimensions: 384,
        pooling: "cls".into(),
        normalization: "l2".into(),
        instruction_policy: "v1".into(),
        text_format: 1,
        runtime_build: "fixture-runtime".into(),
    };
    let mut second = first.clone();
    second.instruction_policy = "v2".into();
    assert_ne!(first, second);
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}

#[test]
fn encoder_identity_storage_key_changes_when_model_id_changes() {
    let first = fixture_identity();
    let mut second = first.clone();
    second.model_id = "qwen3-0.6b-f16".into();
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}

#[test]
fn encoder_identity_storage_key_changes_when_weights_hash_changes() {
    let first = fixture_identity();
    let mut second = first.clone();
    second.weights_sha256 = "b".repeat(64);
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}

#[test]
fn encoder_identity_storage_key_changes_when_dimensions_change() {
    let first = fixture_identity();
    let mut second = first.clone();
    second.dimensions = 512;
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}

#[test]
fn encoder_identity_storage_key_changes_when_pooling_changes() {
    let first = fixture_identity();
    let mut second = first.clone();
    second.pooling = "mean".into();
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}

#[test]
fn encoder_identity_storage_key_changes_when_normalization_changes() {
    let first = fixture_identity();
    let mut second = first.clone();
    second.normalization = "none".into();
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}

#[test]
fn encoder_identity_storage_key_changes_when_text_format_changes() {
    let first = fixture_identity();
    let mut second = first.clone();
    second.text_format = 2;
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}

#[test]
fn encoder_identity_storage_key_changes_when_runtime_build_changes() {
    let first = fixture_identity();
    let mut second = first.clone();
    second.runtime_build = "llama.cpp-b3560".into();
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}

#[test]
fn encoder_identity_storage_key_changes_when_schema_changes() {
    let first = fixture_identity();
    let mut second = first.clone();
    second.schema = 2;
    assert_ne!(first.storage_key().unwrap(), second.storage_key().unwrap());
}

#[test]
fn encoder_identity_validation_rejects_invalid_fields() {
    let base = fixture_identity();

    // Zero schema
    let mut invalid = base.clone();
    invalid.schema = 0;
    assert!(invalid.storage_key().is_err());

    // Empty model_id
    let mut invalid = base.clone();
    invalid.model_id = "   ".into();
    assert!(invalid.storage_key().is_err());

    // Short weights checksum (< 64 chars)
    let mut invalid = base.clone();
    invalid.weights_sha256 = "a".repeat(63);
    assert!(invalid.storage_key().is_err());

    // Non-hex weights checksum
    let mut invalid = base.clone();
    invalid.weights_sha256 = "g".repeat(64);
    assert!(invalid.storage_key().is_err());

    // Zero dimensions
    let mut invalid = base.clone();
    invalid.dimensions = 0;
    assert!(invalid.storage_key().is_err());

    // Empty pooling
    let mut invalid = base.clone();
    invalid.pooling = "".into();
    assert!(invalid.storage_key().is_err());

    // Unknown normalization
    let mut invalid = base.clone();
    invalid.normalization = "bogus".into();
    assert!(invalid.storage_key().is_err());

    // Empty instruction policy
    let mut invalid = base.clone();
    invalid.instruction_policy = "".into();
    assert!(invalid.storage_key().is_err());

    // Zero text format
    let mut invalid = base.clone();
    invalid.text_format = 0;
    assert!(invalid.storage_key().is_err());

    // Empty runtime build
    let mut invalid = base.clone();
    invalid.runtime_build = "".into();
    assert!(invalid.storage_key().is_err());
}

#[test]
fn embedding_request_budget_enforces_cancellation_and_deadline() {
    // Cancellation check
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(10));
    assert!(budget.check_budget().is_ok());
    budget.cancel();
    assert!(budget.is_cancelled());
    assert!(budget.check_budget().is_err());

    // Deadline expiration check
    let expired_budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(1));
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(expired_budget.remaining_time(), Duration::ZERO);
    assert!(expired_budget.remaining_time_checked().is_none());
    assert!(expired_budget.check_budget().is_err());
}

#[test]
fn challenge_hex_case_variations_and_normalization() {
    let lower_hash = "abcdef0123456789".repeat(4);
    let upper_hash = "ABCDEF0123456789".repeat(4);
    let mixed_hash = "AbCdEf0123456789".repeat(4);

    let mut id_lower = fixture_identity();
    id_lower.weights_sha256 = lower_hash;

    let mut id_upper = fixture_identity();
    id_upper.weights_sha256 = upper_hash;

    let mut id_mixed = fixture_identity();
    id_mixed.weights_sha256 = mixed_hash;

    // All must pass validation
    assert!(id_lower.validate().is_ok());
    assert!(id_upper.validate().is_ok());
    assert!(id_mixed.validate().is_ok());

    // Storage keys MUST be identical because hex hash is normalized to lowercase
    let key_lower = id_lower.storage_key().unwrap();
    let key_upper = id_upper.storage_key().unwrap();
    let key_mixed = id_mixed.storage_key().unwrap();

    assert_eq!(
        key_lower, key_upper,
        "uppercase hex must produce identical storage_key"
    );
    assert_eq!(
        key_lower, key_mixed,
        "mixed-case hex must produce identical storage_key"
    );

    // Rust struct equality is case-sensitive, contrasting with storage_key canonicalization
    assert_ne!(id_lower, id_upper, "in-memory PartialEq is case-sensitive");
    assert_ne!(id_lower, id_mixed, "in-memory PartialEq is case-sensitive");

    // Normalization case variations (e.g. "l2" vs "L2", "none" vs "NONE")
    let mut id_norm_lower = fixture_identity();
    id_norm_lower.normalization = "l2".into();
    let mut id_norm_upper = fixture_identity();
    id_norm_upper.normalization = "L2".into();

    assert!(id_norm_lower.validate().is_ok());
    assert!(id_norm_upper.validate().is_ok());
    assert_eq!(
        id_norm_lower.storage_key().unwrap(),
        id_norm_upper.storage_key().unwrap(),
        "normalization case variations must normalize to identical storage_key"
    );
}

#[test]
fn challenge_single_field_sensitivity_all_fields() {
    let base = fixture_identity();
    let base_key = base.storage_key().unwrap();

    // 1. schema
    let mut m_schema = base.clone();
    m_schema.schema = 2;
    assert_ne!(
        base_key,
        m_schema.storage_key().unwrap(),
        "schema must change storage_key"
    );

    // 2. model_id
    let mut m_model = base.clone();
    m_model.model_id = "qwen3-0.6b-f16".into();
    assert_ne!(
        base_key,
        m_model.storage_key().unwrap(),
        "model_id must change storage_key"
    );

    // 3. weights_sha256
    let mut m_weights = base.clone();
    m_weights.weights_sha256 = "b".repeat(64);
    assert_ne!(
        base_key,
        m_weights.storage_key().unwrap(),
        "weights_sha256 must change storage_key"
    );

    // 4. dimensions
    let mut m_dims = base.clone();
    m_dims.dimensions = 512;
    assert_ne!(
        base_key,
        m_dims.storage_key().unwrap(),
        "dimensions must change storage_key"
    );

    // 5. pooling
    let mut m_pool = base.clone();
    m_pool.pooling = "mean".into();
    assert_ne!(
        base_key,
        m_pool.storage_key().unwrap(),
        "pooling must change storage_key"
    );

    // 6. normalization
    let mut m_norm = base.clone();
    m_norm.normalization = "none".into();
    assert_ne!(
        base_key,
        m_norm.storage_key().unwrap(),
        "normalization must change storage_key"
    );

    // 7. instruction_policy
    let mut m_inst = base.clone();
    m_inst.instruction_policy = "v2".into();
    assert_ne!(
        base_key,
        m_inst.storage_key().unwrap(),
        "instruction_policy must change storage_key"
    );

    // 8. text_format
    let mut m_tf = base.clone();
    m_tf.text_format = 2;
    assert_ne!(
        base_key,
        m_tf.storage_key().unwrap(),
        "text_format must change storage_key"
    );

    // 9. runtime_build
    let mut m_rb = base.clone();
    m_rb.runtime_build = "llama.cpp-b3560".into();
    assert_ne!(
        base_key,
        m_rb.storage_key().unwrap(),
        "runtime_build must change storage_key"
    );

    // Adversarial JSON delimiter collision attempt:
    // Injecting JSON syntax into a string field must NOT collide with a genuine identity
    let mut m_inject = base.clone();
    m_inject.model_id = format!("{}\",\"pooling\":\"injected", base.model_id);
    let inject_key = m_inject.storage_key().unwrap();
    assert_ne!(base_key, inject_key);
}

#[test]
fn challenge_invalid_hex_digest_comprehensive() {
    let base = fixture_identity();

    let invalid_cases = [
        ("63 chars (off-by-one under)", "a".repeat(63)),
        ("65 chars (off-by-one over)", "a".repeat(65)),
        ("empty string (0 chars)", "".to_string()),
        (
            "64 chars with non-hex 'g' at start",
            format!("g{}", "a".repeat(63)),
        ),
        (
            "64 chars with non-hex 'g' in middle",
            format!("{}g{}", "a".repeat(32), "a".repeat(31)),
        ),
        (
            "64 chars with non-hex 'g' at end",
            format!("{}g", "a".repeat(63)),
        ),
        ("64 chars with 'z'", "z".repeat(64)),
        ("64 chars with spaces", " ".repeat(64)),
        (
            "64 chars with trailing space",
            format!("{} ", "a".repeat(63)),
        ),
        (
            "64 chars with leading space",
            format!(" {}", "a".repeat(63)),
        ),
        ("64 chars with newline", format!("{}\n", "a".repeat(63))),
        ("64 chars with tab", format!("{}\t", "a".repeat(63))),
        ("64 chars with punctuation '!'", "!".repeat(64)),
        ("64 chars with punctuation '-'", "-".repeat(64)),
        ("64 chars with null byte", format!("{}\0", "0".repeat(63))),
        ("128 chars (SHA-512)", "a".repeat(128)),
    ];

    for (desc, invalid_hash) in invalid_cases {
        let mut id = base.clone();
        id.weights_sha256 = invalid_hash;
        assert!(id.validate().is_err(), "validate() should fail for {desc}");
        assert!(
            id.storage_key().is_err(),
            "storage_key() should fail for {desc}"
        );
    }
}

#[test]
fn challenge_embedding_request_budget_cancellation_and_concurrency() {
    let external_flag = Arc::new(AtomicBool::new(false));
    let budget = EmbeddingRequestBudget::new(
        Instant::now() + Duration::from_secs(60),
        external_flag.clone(),
    );

    assert!(!budget.is_cancelled());
    assert!(budget.check_budget().is_ok());

    // Setting external flag directly updates budget.is_cancelled()
    external_flag.store(true, Ordering::Release);
    assert!(budget.is_cancelled());
    assert!(budget.check_budget().is_err());

    // Reset and test cancel() method
    external_flag.store(false, Ordering::Release);
    assert!(!budget.is_cancelled());
    budget.cancel();
    assert!(budget.is_cancelled());
    assert!(external_flag.load(Ordering::Acquire));

    // Multi-threaded observation
    let shared_budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(60));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let b = shared_budget.clone();
        handles.push(std::thread::spawn(move || {
            while !b.is_cancelled() {
                std::hint::spin_loop();
            }
            assert!(b.check_budget().is_err());
        }));
    }

    std::thread::sleep(Duration::from_millis(10));
    shared_budget.cancel();

    for h in handles {
        h.join().expect("thread join failed");
    }
}

#[test]
fn challenge_embedding_request_budget_deadline_and_remaining_time() {
    // 1. Past deadline (already expired at construction)
    let past_deadline = Instant::now() - Duration::from_secs(2);
    let expired_budget =
        EmbeddingRequestBudget::new(past_deadline, Arc::new(AtomicBool::new(false)));

    assert!(expired_budget.is_expired(), "past deadline must be expired");
    assert_eq!(
        expired_budget.remaining_time(),
        Duration::ZERO,
        "remaining_time() must return Duration::ZERO when expired"
    );
    assert_eq!(
        expired_budget.remaining_time_checked(),
        None,
        "remaining_time_checked() must return None when expired"
    );
    assert_eq!(
        expired_budget.remaining(),
        None,
        "remaining() alias must return None when expired"
    );
    assert!(expired_budget.check_budget().is_err());

    // 2. Future deadline (unexpired)
    let future_budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(30));
    assert!(!future_budget.is_expired());
    assert!(future_budget.remaining_time() > Duration::from_secs(20));
    assert!(future_budget.remaining_time_checked().is_some());
    assert!(future_budget.remaining().is_some());
    assert!(future_budget.check_budget().is_ok());

    // 3. Unbounded budget
    let unbounded = EmbeddingRequestBudget::unbounded();
    assert!(!unbounded.is_expired());
    assert!(!unbounded.is_cancelled());
    assert!(unbounded.remaining_time() > Duration::from_secs(365 * 24 * 3600 - 100));
    assert!(unbounded.check_budget().is_ok());
}

#[test]
fn challenge_embedding_request_budget_check_budget_error_types() {
    // 1. Neither cancelled nor expired -> Ok
    let fresh = EmbeddingRequestBudget::with_timeout(Duration::from_secs(60));
    assert!(fresh.check_budget().is_ok());
    assert!(fresh.check().is_ok());

    // 2. Cancelled only -> Err("cancelled")
    let cancelled_only = EmbeddingRequestBudget::with_timeout(Duration::from_secs(60));
    cancelled_only.cancel();
    let err = cancelled_only.check_budget().unwrap_err();
    assert!(
        err.to_string().contains("cancelled"),
        "error message should mention cancelled, got: {err}"
    );

    // 3. Expired only -> Err("deadline exceeded")
    let past_deadline = Instant::now() - Duration::from_millis(50);
    let expired_only = EmbeddingRequestBudget::new(past_deadline, Arc::new(AtomicBool::new(false)));
    let err = expired_only.check_budget().unwrap_err();
    assert!(
        err.to_string().contains("deadline exceeded"),
        "error message should mention deadline exceeded, got: {err}"
    );

    // 4. Both cancelled and expired -> cancellation is checked first
    let both = EmbeddingRequestBudget::new(past_deadline, Arc::new(AtomicBool::new(true)));
    let err = both.check_budget().unwrap_err();
    assert!(
        err.to_string().contains("cancelled"),
        "cancellation takes precedence over deadline, got: {err}"
    );
}
