use std::io::{self, BufReader, Read, Write};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use tempfile::tempdir;

#[cfg(unix)]
use crate::embeddings::native::NativeEmbeddingProvider;
#[cfg(unix)]
use crate::embeddings::native::client::NativeClientConn;
#[cfg(unix)]
use crate::embeddings::native::launch::{BrokerPaths, NativeLaunchConfig};
#[cfg(unix)]
use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity,
};

use crate::embeddings::native::client::{
    MAX_PAYLOAD_BYTES, decode_native_batch_reply, decode_native_health_reply,
    decode_native_query_reply, read_line_bounded, read_line_bounded_deadline,
};
use crate::embeddings::native::health::validate_native_health;
use crate::embeddings::sidecar_protocol::HealthResult;

// Helper stream yielding infinite bytes without newlines
struct InfiniteByteStream(u8);

impl Read for InfiniteByteStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        buf.fill(self.0);
        Ok(buf.len())
    }
}

// Helper stream generating a line of exact length ending with newline
struct ExactLenStream {
    total_len: usize,
    pos: usize,
}

impl ExactLenStream {
    fn new(total_len: usize) -> Self {
        Self { total_len, pos: 0 }
    }
}

impl Read for ExactLenStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.total_len {
            return Ok(0);
        }
        let available = self.total_len - self.pos;
        let to_write = buf.len().min(available);
        for i in 0..to_write {
            if self.pos + i == self.total_len - 1 {
                buf[i] = b'\n';
            } else {
                buf[i] = b'a';
            }
        }
        self.pos += to_write;
        Ok(to_write)
    }
}

// =========================================================================
// 1. Float Invariants: NaN, +Inf, -Inf
// =========================================================================

#[test]
fn challenge_query_reply_rejects_positive_infinity() {
    // 1e300 in f32 overflows to +Infinity
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vector":[0.5,1e300]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("non-finite float"),
        "expected non-finite error, got: {err}"
    );
}

#[test]
fn challenge_query_reply_rejects_negative_infinity() {
    // -1e300 in f32 overflows to -Infinity
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vector":[-1e300,0.5]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("non-finite float"),
        "expected non-finite error, got: {err}"
    );
}

#[test]
fn challenge_query_reply_rejects_nan_string_token() {
    // String "NaN" in float vector must fail serde parsing
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vector":[0.5,"NaN"]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("invalid json"),
        "expected json error, got: {err}"
    );
}

#[test]
fn challenge_query_reply_rejects_null_in_vector() {
    // null in vector must fail serde parsing
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vector":[0.5,null]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("invalid json"),
        "expected json error, got: {err}"
    );
}

#[test]
fn challenge_batch_reply_rejects_negative_infinity_in_second_vector() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vectors":[[0.1,0.2],[0.3,-1e300]]},"error":null}"#;
    let result = decode_native_batch_reply(bytes, "req-1", 2, 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("non-finite float in vector 1 at index 1"),
        "got: {err}"
    );
}

#[test]
fn challenge_batch_reply_rejects_ragged_vectors() {
    // Vector 0 has 2 elements, Vector 1 has 3 elements (expected dims: 2)
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vectors":[[0.1,0.2],[0.3,0.4,0.5]]},"error":null}"#;
    let result = decode_native_batch_reply(bytes, "req-1", 2, 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("vector length mismatch at index 1"),
        "got: {err}"
    );
}

// =========================================================================
// 2. Strict Envelope Validation: Schema, Version, Request ID
// =========================================================================

#[test]
fn challenge_batch_reply_rejects_schema_mismatch() {
    let bytes = br#"{"schema":"wrong.schema","version":1,"request_id":"req-1","result":{"dims":2,"vectors":[[0.1,0.2]]},"error":null}"#;
    let result = decode_native_batch_reply(bytes, "req-1", 2, 1);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("schema mismatch"), "got: {err}");
}

#[test]
fn challenge_batch_reply_rejects_version_mismatch() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":0,"request_id":"req-1","result":{"dims":2,"vectors":[[0.1,0.2]]},"error":null}"#;
    let result = decode_native_batch_reply(bytes, "req-1", 2, 1);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("version mismatch"), "got: {err}");
}

#[test]
fn challenge_health_reply_rejects_schema_mismatch() {
    let bytes = br#"{"schema":"wrong.schema","version":1,"request_id":"h-1","result":{"ready":true,"dims":384},"error":null}"#;
    let result = decode_native_health_reply(bytes, "h-1");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("schema mismatch"), "got: {err}");
}

#[test]
fn challenge_health_reply_rejects_version_mismatch() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":99,"request_id":"h-1","result":{"ready":true,"dims":384},"error":null}"#;
    let result = decode_native_health_reply(bytes, "h-1");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("version mismatch"), "got: {err}");
}

#[test]
fn challenge_health_reply_rejects_request_id_mismatch() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"h-unexpected","result":{"ready":true,"dims":384},"error":null}"#;
    let result = decode_native_health_reply(bytes, "h-expected");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("request_id mismatch"), "got: {err}");
}

#[test]
fn challenge_query_reply_accepts_id_alias() {
    // Sidecar may send "id" instead of "request_id"
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"id":"req-alias","result":{"dims":2,"vector":[0.1,0.2]},"error":null}"#;
    let result =
        decode_native_query_reply(bytes, "req-alias", 2).expect("id alias should be accepted");
    assert_eq!(result, vec![0.1, 0.2]);
}

// =========================================================================
// 3. Mutual Exclusivity of Result and Error
// =========================================================================

#[test]
fn challenge_batch_reply_rejects_both_result_and_error() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":{"dims":2,"vectors":[[0.1,0.2]]},"error":{"code":"err","message":"msg"}}"#;
    let result = decode_native_batch_reply(bytes, "req-1", 2, 1);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("invariant violation"), "got: {err}");
}

#[test]
fn challenge_batch_reply_rejects_neither_result_nor_error() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","result":null,"error":null}"#;
    let result = decode_native_batch_reply(bytes, "req-1", 2, 1);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("invariant violation"), "got: {err}");
}

#[test]
fn challenge_health_reply_rejects_both_result_and_error() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"h-1","result":{"ready":true,"dims":384},"error":{"code":"err","message":"msg"}}"#;
    let result = decode_native_health_reply(bytes, "h-1");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("invariant violation"), "got: {err}");
}

#[test]
fn challenge_health_reply_rejects_neither_result_nor_error() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"h-1","result":null,"error":null}"#;
    let result = decode_native_health_reply(bytes, "h-1");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("invariant violation"), "got: {err}");
}

#[test]
fn challenge_query_reply_rejects_omitted_result_and_error_keys() {
    // Both "result" and "error" keys are entirely missing from the JSON object
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1"}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("invariant violation"), "got: {err}");
}

// =========================================================================
// 4. Framing & Line Length Limits (read_line_bounded)
// =========================================================================

#[test]
fn challenge_read_line_bounded_rejects_infinite_stream_without_memory_exhaustion() {
    let mut reader = BufReader::new(InfiniteByteStream(b'z'));
    let mut out = Vec::new();
    let result = read_line_bounded(&mut reader, &mut out, MAX_PAYLOAD_BYTES);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(
        out.len() <= MAX_PAYLOAD_BYTES,
        "buffer allocated {} bytes, exceeding MAX_PAYLOAD_BYTES {}",
        out.len(),
        MAX_PAYLOAD_BYTES
    );
}

#[test]
fn challenge_read_line_bounded_boundary_at_exact_max_bytes() {
    // Exactly MAX_PAYLOAD_BYTES ending with newline
    let stream = ExactLenStream::new(MAX_PAYLOAD_BYTES);
    let mut reader = BufReader::new(stream);
    let mut out = Vec::new();
    let n = read_line_bounded(&mut reader, &mut out, MAX_PAYLOAD_BYTES)
        .expect("exact limit should succeed");
    assert_eq!(n, MAX_PAYLOAD_BYTES);
    assert_eq!(out.len(), MAX_PAYLOAD_BYTES);
    assert_eq!(out.last(), Some(&b'\n'));
}

#[test]
fn challenge_read_line_bounded_boundary_at_max_bytes_plus_one() {
    // MAX_PAYLOAD_BYTES + 1 ending with newline
    let stream = ExactLenStream::new(MAX_PAYLOAD_BYTES + 1);
    let mut reader = BufReader::new(stream);
    let mut out = Vec::new();
    let result = read_line_bounded(&mut reader, &mut out, MAX_PAYLOAD_BYTES);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("exceeds maximum allowed size"));
}

// =========================================================================
// 5. Malformed Framing & Extra Fields
// =========================================================================

#[test]
fn challenge_query_reply_tolerates_extra_fields() {
    // Unknown envelope metadata (e.g. forward-compatible telemetry/timing) should not break decoding
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1,"request_id":"req-1","trace_id":"xyz-123","result":{"dims":2,"vector":[0.1,0.2]},"error":null}"#;
    let result = decode_native_query_reply(bytes, "req-1", 2)
        .expect("forward-compatible extra fields should be allowed");
    assert_eq!(result, vec![0.1, 0.2]);
}

#[test]
fn challenge_query_reply_rejects_truncated_json() {
    let bytes = br#"{"schema":"julie.embedding.sidecar","version":1"#;
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("invalid json"), "got: {err}");
}

#[test]
fn challenge_query_reply_rejects_garbage_http_protocol() {
    let bytes = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n";
    let result = decode_native_query_reply(bytes, "req-1", 2);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("invalid json"), "got: {err}");
}

#[test]
fn challenge_validate_native_health_rejects_invalid_weights_sha256() {
    // Non-hex or wrong length SHA-256 must be rejected by EncoderIdentity validation
    let health = HealthResult {
        ready: true,
        dims: Some(384),
        model_id: Some("bge-small-en-v1.5-f32".to_string()),
        model_sha256: Some("not-a-valid-sha256".to_string()),
        pooling: Some("cls".to_string()),
        normalization: Some("l2".to_string()),
        instruction_policy_version: Some(1),
        llama_cpp_build: Some("b3560".to_string()),
        ..Default::default()
    };
    let result = validate_native_health(&health, Some("bge-small-en-v1.5-f32"));
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("weights_sha256"),
        "expected weights_sha256 error, got: {err}"
    );
}

#[test]
fn challenge_validate_native_health_rejects_missing_pooling() {
    let health = HealthResult {
        ready: true,
        dims: Some(384),
        model_id: Some("bge-small-en-v1.5-f32".to_string()),
        model_sha256: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        ),
        pooling: None,
        normalization: Some("l2".to_string()),
        instruction_policy_version: Some(1),
        llama_cpp_build: Some("b3560".to_string()),
        ..Default::default()
    };
    let result = validate_native_health(&health, Some("bge-small-en-v1.5-f32"));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("missing pooling"));
}

#[test]
fn challenge_validate_native_health_rejects_missing_normalization() {
    let health = HealthResult {
        ready: true,
        dims: Some(384),
        model_id: Some("bge-small-en-v1.5-f32".to_string()),
        model_sha256: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        ),
        pooling: Some("cls".to_string()),
        normalization: None,
        instruction_policy_version: Some(1),
        llama_cpp_build: Some("b3560".to_string()),
        ..Default::default()
    };
    let result = validate_native_health(&health, Some("bge-small-en-v1.5-f32"));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("missing normalization")
    );
}

#[test]
fn challenge_validate_native_health_rejects_mismatched_model_id() {
    let health = HealthResult {
        ready: true,
        dims: Some(384),
        model_id: Some("different-model-id".to_string()),
        model_sha256: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        ),
        pooling: Some("cls".to_string()),
        normalization: Some("l2".to_string()),
        instruction_policy_version: Some(1),
        llama_cpp_build: Some("b3560".to_string()),
        ..Default::default()
    };
    let result = validate_native_health(&health, Some("bge-small-en-v1.5-f32"));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("model_id mismatch")
    );
}

#[test]
fn challenge_validate_native_health_rejects_missing_instruction_policy() {
    let health = HealthResult {
        ready: true,
        dims: Some(384),
        model_id: Some("bge-small-en-v1.5-f32".to_string()),
        model_sha256: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        ),
        pooling: Some("cls".to_string()),
        normalization: Some("l2".to_string()),
        instruction_policy_version: None,
        llama_cpp_build: Some("b3560".to_string()),
        ..Default::default()
    };
    let result = validate_native_health(&health, Some("bge-small-en-v1.5-f32"));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("missing instruction_policy_version")
    );
}

#[test]
fn challenge_validate_native_health_rejects_missing_llama_build() {
    let health = HealthResult {
        ready: true,
        dims: Some(384),
        model_id: Some("bge-small-en-v1.5-f32".to_string()),
        model_sha256: Some(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        ),
        pooling: Some("cls".to_string()),
        normalization: Some("l2".to_string()),
        instruction_policy_version: Some(1),
        llama_cpp_build: None,
        ..Default::default()
    };
    let result = validate_native_health(&health, Some("bge-small-en-v1.5-f32"));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("missing llama_cpp_build")
    );
}

// =========================================================================
// 6. Milestone M3 (Finding 7) Absolute Transport Deadlines & Trickling Bytes
// =========================================================================

// Helper stream yielding bytes slowly (trickling 1 byte per delay)
struct TricklingByteStream {
    byte_delay: Duration,
    total_bytes: usize,
    emitted: usize,
    byte_value: u8,
}

impl TricklingByteStream {
    fn new(total_bytes: usize, byte_delay: Duration, byte_value: u8) -> Self {
        Self {
            byte_delay,
            total_bytes,
            emitted: 0,
            byte_value,
        }
    }
}

impl Read for TricklingByteStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.emitted >= self.total_bytes || buf.is_empty() {
            return Ok(0);
        }
        std::thread::sleep(self.byte_delay);
        buf[0] = self.byte_value;
        self.emitted += 1;
        Ok(1)
    }
}

#[cfg(unix)]
fn mock_identity(model_id: &str, dims: usize) -> EncoderIdentity {
    EncoderIdentity {
        schema: 1,
        model_id: model_id.to_string(),
        weights_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_string(),
        dimensions: dims,
        pooling: "cls".to_string(),
        normalization: "l2".to_string(),
        instruction_policy: "v1".to_string(),
        text_format: 1,
        runtime_build: "llama.cpp-b3560".to_string(),
    }
}

#[cfg(unix)]
fn mock_device_info(model_id: &str, dims: usize) -> DeviceInfo {
    DeviceInfo {
        runtime: "llama.cpp".to_string(),
        device: "cpu".to_string(),
        model_name: model_id.to_string(),
        dimensions: dims,
    }
}

#[test]
fn challenge_read_line_bounded_deadline_terminates_trickling_stream_promptly() {
    // 100 bytes at 5ms each would take 500ms if allowed to complete
    let stream = TricklingByteStream::new(100, Duration::from_millis(5), b'a');
    let mut reader = BufReader::new(stream);
    let mut out = Vec::new();

    let deadline = Some(Instant::now() + Duration::from_millis(40));
    let per_read_timeout = Some(Duration::from_millis(200));
    let start = Instant::now();

    let result = read_line_bounded_deadline(
        &mut reader,
        &mut out,
        MAX_PAYLOAD_BYTES,
        deadline,
        per_read_timeout,
        |_| Ok(()),
    );
    let elapsed = start.elapsed();

    assert!(result.is_err(), "expected deadline timeout error");
    let err = result.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    assert!(
        err.to_string().contains("deadline exceeded"),
        "expected 'deadline exceeded', got: {err}"
    );
    assert!(
        elapsed < Duration::from_millis(250),
        "trickling stream read should terminate promptly upon deadline expiry (took {:?})",
        elapsed
    );
    assert!(
        elapsed >= Duration::from_millis(30),
        "should not terminate prematurely before deadline (took {:?})",
        elapsed
    );
    assert!(
        out.len() < 100,
        "should have aborted before reading all 100 bytes (read {} bytes)",
        out.len()
    );
}

#[cfg(unix)]
#[test]
fn challenge_read_line_bounded_deadline_trickling_unix_socket_terminates_promptly() {
    use std::os::unix::net::UnixStream;

    let (mut tx, rx) = UnixStream::pair().unwrap();
    let rx_stream = rx.try_clone().unwrap();
    let mut reader = BufReader::new(rx);

    let sender = std::thread::spawn(move || {
        // Trickle 100 bytes, 1 byte every 6ms (total 600ms)
        for _ in 0..100 {
            if tx.write_all(b"x").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(6));
        }
    });

    let mut out = Vec::new();
    let deadline = Some(Instant::now() + Duration::from_millis(40));
    let per_read = Some(Duration::from_millis(200));
    let start = Instant::now();

    let result = read_line_bounded_deadline(
        &mut reader,
        &mut out,
        MAX_PAYLOAD_BYTES,
        deadline,
        per_read,
        |t| rx_stream.set_read_timeout(t),
    );
    let elapsed = start.elapsed();

    assert!(result.is_err(), "expected timeout error");
    let err = result.unwrap_err();
    // EMPIRICAL CHALLENGER FINDING:
    // On Unix, SO_RCVTIMEO expiration returns EAGAIN/EWOULDBLOCK (ErrorKind::WouldBlock).
    // Because read_line_bounded_deadline does `reader.fill_buf()?`, it propagates WouldBlock
    // rather than mapping it to io::ErrorKind::TimedOut.
    assert!(
        err.kind() == io::ErrorKind::TimedOut || err.kind() == io::ErrorKind::WouldBlock,
        "expected TimedOut or WouldBlock, got {:?}",
        err.kind()
    );
    assert!(
        elapsed < Duration::from_millis(250),
        "trickling socket read should terminate promptly on deadline expiry (took {:?})",
        elapsed
    );
    assert!(
        out.len() < 100,
        "read {} bytes, should have stopped before full 100 bytes",
        out.len()
    );

    let _ = sender.join();
}

#[cfg(unix)]
fn compile_mock_broker(dir: &std::path::Path) -> std::path::PathBuf {
    let src_path = dir.join("mock_broker.rs");
    let bin_path = dir.join("mock_broker_bin");
    if bin_path.exists() {
        return bin_path;
    }
    let src = r#"
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::thread;
use std::time::Duration;

fn main() {
    let endpoint = env::var("MOCK_ENDPOINT").expect("MOCK_ENDPOINT");
    let health_delay: u64 = env::var("HEALTH_DELAY_MS").unwrap_or_default().parse().unwrap_or(0);
    let query_delay: u64 = env::var("QUERY_DELAY_MS").unwrap_or_default().parse().unwrap_or(0);
    let drop_first = env::var("DROP_FIRST").map(|v| v == "1").unwrap_or(false);
    let query_flag_path = env::var("QUERY_FLAG_PATH").ok();

    let listener = UnixListener::bind(Path::new(&endpoint)).expect("bind");
    let mut is_first = true;
    for stream in listener.incoming() {
        if let Ok(mut stream) = stream {
            if is_first && drop_first {
                is_first = false;
                drop(stream);
                continue;
            }
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if line.trim().is_empty() {
                    line.clear();
                    continue;
                }
                let req_id = if let Some(pos) = line.find("\"request_id\":") {
                    let rest = &line[pos + 13..];
                    if let Some(start) = rest.find('"') {
                        if let Some(end) = rest[start + 1..].find('"') {
                            &rest[start + 1..start + 1 + end]
                        } else { "req-1" }
                    } else { "req-1" }
                } else { "req-1" };

                if line.contains("\"health\"") {
                    if health_delay > 0 {
                        thread::sleep(Duration::from_millis(health_delay));
                    }
                    let reply = format!("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"{req_id}\",\"result\":{{\"ready\":true,\"dims\":384,\"device\":\"cpu\",\"runtime\":\"llama.cpp\",\"model_id\":\"bge-small-en-v1.5-f32\",\"model_sha256\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\",\"pooling\":\"cls\",\"normalization\":\"l2\",\"instruction_policy_version\":1,\"llama_cpp_build\":\"b3560\"}},\"error\":null}}\n");
                    let _ = stream.write_all(reply.as_bytes());
                    let _ = stream.flush();
                } else if line.contains("\"embed_query\"") {
                    if let Some(ref flag) = query_flag_path {
                        let _ = std::fs::write(flag, b"1");
                    }
                    if query_delay > 0 {
                        thread::sleep(Duration::from_millis(query_delay));
                    }
                    let mut reply = format!("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"{req_id}\",\"result\":{{\"dims\":384,\"vector\":[");
                    for idx in 0..384 {
                        if idx > 0 { reply.push(','); }
                        reply.push_str("0.42");
                    }
                    reply.push_str("]},\"error\":null}\n");
                    let _ = stream.write_all(reply.as_bytes());
                    let _ = stream.flush();
                }
                line.clear();
            }
        }
    }
}
"#;
    std::fs::write(&src_path, src).unwrap();
    let status = std::process::Command::new("rustc")
        .args([src_path.to_str().unwrap(), "-o", bin_path.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success(), "failed to compile mock broker");
    bin_path
}

#[cfg(unix)]
#[test]
fn challenge_budget_deduction_after_broker_connection_subtracts_elapsed_time() {
    let tmp = tempdir().unwrap();
    let bin_path = compile_mock_broker(tmp.path());
    let sock_path = tmp.path().join("deduct_timeout.sock");

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: bin_path.clone(),
        executable_sha256: "mock".to_string(),
        model_id: "bge-small-en-v1.5-f32".to_string(),
        cache_root: tmp.path().to_path_buf(),
        broker_paths: paths.clone(),
    };

    let mut child = std::process::Command::new(&bin_path)
        .env("MOCK_ENDPOINT", &sock_path)
        .env("DROP_FIRST", "1")
        .env("HEALTH_DELAY_MS", "45")
        .env("QUERY_DELAY_MS", "70")
        .spawn()
        .expect("spawn mock broker");

    for _ in 0..100 {
        if sock_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }

    let client = NativeClientConn::connect(&sock_path, Some(Duration::from_millis(500))).unwrap();
    let identity = mock_identity("bge-small-en-v1.5-f32", 384);
    let device = mock_device_info("bge-small-en-v1.5-f32", 384);
    let provider = NativeEmbeddingProvider::from_connected(config, client, identity, device);

    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(90));
    let start = Instant::now();
    let res = provider.embed_query("deduct test", &budget);
    let elapsed = start.elapsed();

    assert!(
        res.is_err(),
        "request should have timed out due to budget deduction"
    );
    let err_str = res.unwrap_err().to_string();
    assert!(
        err_str.contains("deadline exceeded")
            || err_str.contains("timed out")
            || err_str.contains("TimedOut")
            || err_str.contains("Resource temporarily unavailable"),
        "expected timeout/deadline exceeded or WouldBlock, got: {err_str}"
    );
    assert!(
        elapsed < Duration::from_millis(150),
        "total execution time was {:?}, which allowed the full delayed server reply",
        elapsed
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
#[test]
fn challenge_budget_exhausted_during_connection_aborts_without_round_trip() {
    let tmp = tempdir().unwrap();
    let bin_path = compile_mock_broker(tmp.path());
    let sock_path = tmp.path().join("exhaust_timeout.sock");
    let flag_path = tmp.path().join("query_received.flag");

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: bin_path.clone(),
        executable_sha256: "mock".to_string(),
        model_id: "bge-small-en-v1.5-f32".to_string(),
        cache_root: tmp.path().to_path_buf(),
        broker_paths: paths.clone(),
    };

    let mut child = std::process::Command::new(&bin_path)
        .env("MOCK_ENDPOINT", &sock_path)
        .env("DROP_FIRST", "1")
        .env("HEALTH_DELAY_MS", "50")
        .env("QUERY_FLAG_PATH", &flag_path)
        .spawn()
        .expect("spawn mock broker");

    for _ in 0..100 {
        if sock_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }

    let client = NativeClientConn::connect(&sock_path, Some(Duration::from_millis(500))).unwrap();
    let identity = mock_identity("bge-small-en-v1.5-f32", 384);
    let device = mock_device_info("bge-small-en-v1.5-f32", 384);
    let provider = NativeEmbeddingProvider::from_connected(config, client, identity, device);

    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(35));
    let res = provider.embed_query("exhaust test", &budget);

    assert!(
        res.is_err(),
        "request should have timed out during reconnect"
    );
    let err_str = res.unwrap_err().to_string();
    assert!(
        err_str.contains("deadline exceeded") || err_str.contains("timed out"),
        "expected deadline exceeded, got: {err_str}"
    );

    assert!(
        !flag_path.exists(),
        "embed_query should not have been dispatched after budget was exhausted during connection"
    );

    let _ = child.kill();
    let _ = child.wait();
}
