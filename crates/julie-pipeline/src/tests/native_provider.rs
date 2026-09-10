use std::io::Cursor;
use std::path::Path;

use julie_core::embeddings_contract::EmbeddingBackend;

use crate::embeddings::factory::{
    BackendResolverCapabilities, parse_provider_preference, resolve_backend_preference,
};
use crate::embeddings::native::client::{
    decode_native_batch_reply, decode_native_health_reply, decode_native_query_reply,
    is_connection_dropped, read_line_bounded,
};
use crate::embeddings::native::health::validate_native_health;
use crate::embeddings::native::launch::derive_broker_paths;
use crate::embeddings::sidecar_protocol::HealthResult;

#[test]
fn native_reply_rejects_mismatched_request_id() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"other","result":{"dims":2,"vector":[0.1,0.2]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "wanted", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("mismatch"),
        "expected mismatch error, got: {err}"
    );
}

#[test]
fn native_reply_rejects_schema_mismatch() {
    let bytes = br#"{"schema":"other.schema","version":1,"request_id":"req-1","result":{"dims":2,"vector":[0.1,0.2]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("schema"), "expected schema error, got: {err}");
}

#[test]
fn native_reply_rejects_version_mismatch() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":2,"request_id":"req-1","result":{"dims":2,"vector":[0.1,0.2]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("version"),
        "expected version error, got: {err}"
    );
}

#[test]
fn native_reply_rejects_both_result_and_error() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vector":[0.1,0.2]},"error":{"code":"err","message":"fail"}}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("invariant violation"),
        "expected invariant violation, got: {err}"
    );
}

#[test]
fn native_reply_rejects_neither_result_nor_error() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":null,"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("invariant violation"),
        "expected invariant violation, got: {err}"
    );
}

#[test]
fn native_reply_extracts_sidecar_error() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":null,"error":{"code":"internal_error","message":"ModelNotPrepared"}}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("internal_error"), "expected code, got: {err}");
    assert!(
        err.contains("ModelNotPrepared"),
        "expected message, got: {err}"
    );
}

#[test]
fn native_reply_rejects_dimension_mismatch() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":3,"vector":[0.1,0.2,0.3]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("dimension mismatch"),
        "expected dim mismatch, got: {err}"
    );
}

#[test]
fn native_reply_rejects_vector_len_mismatch() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vector":[0.1,0.2,0.3]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("vector length mismatch"),
        "expected len mismatch, got: {err}"
    );
}

#[test]
fn native_reply_rejects_non_finite_float() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vector":[0.1,1e300]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("non-finite"),
        "expected non-finite, got: {err}"
    );
}

#[test]
fn native_reply_accepts_valid_query_reply() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vector":[0.123,0.456]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2).expect("valid reply should decode");
    assert_eq!(result, vec![0.123, 0.456]);
}

#[test]
fn native_batch_reply_rejects_mismatched_request_id() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"other","result":{"dims":2,"vectors":[[0.1,0.2]]},"error":null}"#;
    let result = decode_native_batch_reply(bytes, "wanted", 2, 1);
    assert!(result.is_err());
}

#[test]
fn native_batch_reply_rejects_count_mismatch() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vectors":[[0.1,0.2]]},"error":null}"#;
    let result = decode_native_batch_reply(bytes, "req-1", 2, 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("batch count mismatch"),
        "expected count mismatch, got: {err}"
    );
}

#[test]
fn native_batch_reply_rejects_dimension_mismatch() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":3,"vectors":[[0.1,0.2,0.3]]},"error":null}"#;
    let result = decode_native_batch_reply(bytes, "req-1", 2, 1);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("dimension mismatch"),
        "expected dim mismatch, got: {err}"
    );
}

#[test]
fn native_batch_reply_rejects_non_finite_float() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vectors":[[0.1,1e300]]},"error":null}"#;
    let result = decode_native_batch_reply(bytes, "req-1", 2, 1);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("non-finite"),
        "expected non-finite, got: {err}"
    );
}

#[test]
fn native_batch_reply_accepts_empty_batch() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vectors":[]},"error":null}"#;
    let result =
        decode_native_batch_reply(bytes, "req-1", 2, 0).expect("empty batch should decode");
    assert!(result.is_empty());
}

#[test]
fn native_batch_reply_accepts_valid_batch_reply() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vectors":[[0.1,0.2],[0.3,0.4]]},"error":null}"#;
    let result =
        decode_native_batch_reply(bytes, "req-1", 2, 2).expect("valid batch should decode");
    assert_eq!(result, vec![vec![0.1, 0.2], vec![0.3, 0.4]]);
}

#[test]
fn native_health_reply_decodes_correctly() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"health-1","result":{"ready":true,"dims":384,"device":"cpu","runtime":"llama.cpp","model_id":"bge-small-en-v1.5-f32","model_sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","pooling":"cls","normalization":"l2","instruction_policy_version":1,"llama_cpp_build":"b3560"},"error":null}"#;
    let health = decode_native_health_reply(bytes, "health-1").expect("health should decode");
    assert!(health.ready);
    assert_eq!(health.dims, Some(384));
    assert_eq!(health.model_id.as_deref(), Some("bge-small-en-v1.5-f32"));

    let (identity, device_info) = validate_native_health(&health, Some("bge-small-en-v1.5-f32"))
        .expect("validation should succeed");
    assert_eq!(identity.model_id, "bge-small-en-v1.5-f32");
    assert_eq!(identity.dimensions, 384);
    assert_eq!(identity.instruction_policy, "v1");
    assert_eq!(identity.runtime_build, "llama.cpp-b3560");
    assert_eq!(device_info.model_name, "bge-small-en-v1.5-f32");
    assert_eq!(device_info.device, "cpu");
}

#[test]
fn validate_native_health_rejects_unprepared_model() {
    let health = HealthResult {
        ready: false,
        degraded_reason: Some("model_not_prepared".to_string()),
        ..Default::default()
    };
    let result = validate_native_health(&health, None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("MODEL_NOT_PREPARED"),
        "expected MODEL_NOT_PREPARED, got: {err}"
    );
}

#[test]
fn read_line_bounded_reads_exact_line() {
    let data = b"line1\nline2\n";
    let mut cursor = Cursor::new(data);
    let mut out = Vec::new();
    let n = read_line_bounded(&mut cursor, &mut out, 1024).expect("read should succeed");
    assert_eq!(n, 6);
    assert_eq!(out, b"line1\n");
}

#[test]
fn read_line_bounded_rejects_line_exceeding_max_bytes() {
    let data = b"very long line without newline\n";
    let mut cursor = Cursor::new(data);
    let mut out = Vec::new();
    let result = read_line_bounded(&mut cursor, &mut out, 10);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("exceeds maximum allowed size"), "got: {err}");
}

#[test]
fn is_connection_dropped_detects_broken_connections() {
    let eof_err = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof");
    assert!(is_connection_dropped(&eof_err));

    let pipe_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken");
    assert!(is_connection_dropped(&pipe_err));

    let reset_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
    assert!(is_connection_dropped(&reset_err));

    let timeout_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout");
    assert!(!is_connection_dropped(&timeout_err));
}

#[test]
fn derive_broker_paths_is_deterministic_and_within_limits() {
    let cache_root = Path::new("/home/test/.cache/julie-semantic");
    let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let model_id = "bge-small-en-v1.5-f32";

    let paths1 = derive_broker_paths(cache_root, sha256, model_id).unwrap();
    let paths2 = derive_broker_paths(cache_root, sha256, model_id).unwrap();
    assert_eq!(paths1, paths2);

    #[cfg(unix)]
    {
        assert!(
            paths1.endpoint_str.len() < 104,
            "Unix domain socket path must be under 104 bytes"
        );
        assert!(paths1.endpoint_path.to_string_lossy().ends_with(".sock"));
    }
    assert!(paths1.service_lock.to_string_lossy().ends_with(".lock"));
    assert!(
        paths1
            .accelerator_lock
            .to_string_lossy()
            .ends_with("accelerator.lock")
    );
}

#[test]
fn parse_provider_preference_accepts_native() {
    assert_eq!(
        parse_provider_preference("native").unwrap(),
        EmbeddingBackend::Native
    );
    assert_eq!(
        parse_provider_preference("NATIVE").unwrap(),
        EmbeddingBackend::Native
    );
    assert_eq!(
        parse_provider_preference("auto").unwrap(),
        EmbeddingBackend::Auto
    );
    assert!(parse_provider_preference("sidecar").is_err());
}

#[test]
fn resolve_backend_preference_resolves_native() {
    let caps = BackendResolverCapabilities::current();
    assert_eq!(
        resolve_backend_preference(EmbeddingBackend::Native, &caps).unwrap(),
        EmbeddingBackend::Native
    );
}
