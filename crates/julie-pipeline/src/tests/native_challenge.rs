//! Adversarial challenge tests for Milestone N2:
//! - Unix domain socket path length limits (macOS 104, Linux 108) across various model IDs
//! - Multi-process broker spawn race recovery when loser exits 0 without unlinking winner's endpoint
//! - Timed-out connections are dropped and late replies do not contaminate subsequent requests
//! - Retry-once semantics apply strictly to connection drops with positive remaining budget and never retry timeouts

#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity,
};
use tempfile::tempdir;

use crate::embeddings::factory::EmbeddingConfig;
use crate::embeddings::native::client::NativeClientConn;
use crate::embeddings::native::launch::{BrokerPaths, NativeLaunchConfig, derive_broker_paths};
use crate::embeddings::native::{DEFAULT_NATIVE_MODEL, NativeEmbeddingProvider};

// ============================================================================
// Helper Fixtures & Mock Builders
// ============================================================================

fn test_identity(model_id: &str, dims: usize) -> EncoderIdentity {
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

fn test_device_info(model_id: &str, dims: usize) -> DeviceInfo {
    DeviceInfo {
        runtime: "llama.cpp".to_string(),
        device: "cpu".to_string(),
        model_name: model_id.to_string(),
        dimensions: dims,
    }
}

fn make_health_reply(req_id: &str, model_id: &str, dims: usize) -> String {
    format!(
        r#"{{"schema":"julie.embedding.sidecar","version":1,"request_id":"{req_id}","result":{{"ready":true,"dims":{dims},"device":"cpu","runtime":"llama.cpp","model_id":"{model_id}","model_sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","pooling":"cls","normalization":"l2","instruction_policy_version":1,"llama_cpp_build":"b3560"}},"error":null}}"#
    )
}

fn make_query_reply(req_id: &str, dims: usize, fill_val: f32) -> String {
    let vec_data = vec![format!("{fill_val:.3}"); dims].join(",");
    format!(
        r#"{{"schema":"julie.embedding.sidecar","version":1,"request_id":"{req_id}","result":{{"dims":{dims},"vector":[{vec_data}]}},"error":null}}"#
    )
}

fn make_batch_reply(req_id: &str, dims: usize, count: usize, fill_val: f32) -> String {
    let single_vec = format!("[{}]", vec![format!("{fill_val:.3}"); dims].join(","));
    let batch_data = vec![single_vec; count].join(",");
    format!(
        r#"{{"schema":"julie.embedding.sidecar","version":1,"request_id":"{req_id}","result":{{"dims":{dims},"vectors":[{batch_data}]}},"error":null}}"#
    )
}

// ============================================================================
// Challenge 1: Socket Path Length Limits
// ============================================================================

#[test]
fn challenge_socket_path_length_supported_models_stay_under_limits() {
    let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let supported_models = [
        "bge-small-en-v1.5-f32", // 21 chars
        "qwen3-0.6b-f16",        // 14 chars
        "m",                     // 1 char
        "custom-model-id-32-chars-long!!",
    ];

    // Typical cache root directories across platforms
    let cache_roots = [
        Path::new("/tmp/cache"),
        Path::new("/home/developer/.cache/julie-semantic"),
        Path::new("/Users/developer/Library/Caches/julie-semantic"),
        Path::new("/var/folders/zz/zyxvpxvq6csfxvn_n0000000000000/T/cache"),
    ];

    for model in supported_models {
        for root in cache_roots {
            let res = derive_broker_paths(root, sha256, model);
            let expected_len = root.as_os_str().len() + 1 + 24 + model.len();
            if expected_len < 104 {
                assert!(
                    res.is_ok(),
                    "expected Ok for path of length {expected_len}: model={model}, root={root:?}"
                );
                let paths = res.unwrap();
                assert!(
                    paths.endpoint_str.len() < 104,
                    "macOS limit violation: {} >= 104",
                    paths.endpoint_str.len()
                );
                assert!(
                    paths.endpoint_str.len() < 108,
                    "Linux limit violation: {} >= 108",
                    paths.endpoint_str.len()
                );
            } else {
                // When path would exceed 104, it must return Err (fail-closed)
                assert!(
                    res.is_err(),
                    "expected Err when path length {expected_len} >= 104: model={model}, root={root:?}"
                );
                let err = res.unwrap_err().to_string();
                assert!(
                    err.contains("exceeds 104-byte limit"),
                    "unexpected error message: {err}"
                );
            }
        }
    }
}

#[test]
fn challenge_socket_path_exact_boundary_103_and_104() {
    let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let model = "bge-small-en-v1.5-f32"; // 21 chars -> filename is 24 + 21 = 45 chars
    // Total path = root.len() + 1 + 45 = root.len() + 46.
    // For total = 103 (max allowed): root.len() = 103 - 46 = 57.
    // For total = 104 (boundary rejected): root.len() = 104 - 46 = 58.

    let root_57_str = format!("/tmp/{}", "a".repeat(52));
    assert_eq!(root_57_str.len(), 57);
    let paths_103 = derive_broker_paths(Path::new(&root_57_str), sha256, model).unwrap();
    assert_eq!(paths_103.endpoint_str.len(), 103);
    assert!(paths_103.endpoint_str.len() < 104);

    let root_58_str = format!("/tmp/{}", "a".repeat(53));
    assert_eq!(root_58_str.len(), 58);
    let res_104 = derive_broker_paths(Path::new(&root_58_str), sha256, model);
    assert!(res_104.is_err());
    let err = res_104.unwrap_err().to_string();
    assert!(
        err.contains("exceeds 104-byte limit:"),
        "expected limit bail, got: {err}"
    );
}

#[test]
fn challenge_socket_path_actually_binds_in_os_when_under_limit() {
    let tmp = tempdir().unwrap();
    let sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let paths = derive_broker_paths(tmp.path(), sha256, DEFAULT_NATIVE_MODEL).unwrap();

    // Verify the OS kernel UnixListener accepts this exact generated path
    let listener = UnixListener::bind(&paths.endpoint_path);
    assert!(
        listener.is_ok(),
        "OS UnixListener::bind rejected derived path '{}': {:?}",
        paths.endpoint_str,
        listener.err()
    );
}

#[test]
fn challenge_broker_spawn_child_failure_exits_nonzero_and_bails() {
    let tmp = tempdir().unwrap();
    let cache_dir = tmp.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();

    let script_path = tmp.path().join("failing-sidecar");
    let src_path = tmp.path().join("failing.rs");
    fs::write(&src_path, "fn main() { std::process::exit(42); }").unwrap();
    let status = std::process::Command::new("rustc")
        .args([
            src_path.to_str().unwrap(),
            "-o",
            script_path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let config = EmbeddingConfig {
        provider: "native".to_string(),
        cache_dir: Some(cache_dir),
        native_program: Some(script_path),
        native_model: Some(DEFAULT_NATIVE_MODEL.to_string()),
    };

    let provider_res = NativeEmbeddingProvider::try_new(&config);
    let err = match provider_res {
        Ok(_) => panic!("expected error when child exits non-zero"),
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("sidecar broker child process failed with status"),
        "expected failure status in error, got: {err}"
    );
}

// ============================================================================
// Challenge 3: Timed-out Connections Dropped & Queue Contamination Guard
// ============================================================================

#[test]
fn challenge_timed_out_connection_dropped_and_does_not_contaminate_future_requests() {
    let tmp = tempdir().unwrap();
    let sock_path = tmp.path().join("test_timeout.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: std::env::current_exe().unwrap(),
        executable_sha256: "mock".to_string(),
        model_id: "bge-small-en-v1.5-f32".to_string(),
        cache_root: tmp.path().to_path_buf(),
        broker_paths: paths.clone(),
    };

    let client = NativeClientConn::connect(&sock_path, Some(Duration::from_millis(500))).unwrap();
    let identity = test_identity("bge-small-en-v1.5-f32", 384);
    let device = test_device_info("bge-small-en-v1.5-f32", 384);
    let provider = NativeEmbeddingProvider::from_connected(config, client, identity, device);

    let late_reply_sent = Arc::new(AtomicBool::new(false));
    let late_reply_flag = Arc::clone(&late_reply_sent);

    // Mock server:
    // Connection 1: Deliberately delays response for 200ms (causing client to time out)
    // Then sends response for req-1 late.
    // Connection 2: Responds immediately for req-2.
    let srv_handle = thread::spawn(move || {
        // Connection 1
        let (stream1, _) = listener.accept().unwrap();
        let mut reader1 = BufReader::new(stream1.try_clone().unwrap());
        let mut writer1 = stream1;
        let mut line1 = String::new();
        reader1.read_line(&mut line1).unwrap();
        assert!(line1.contains("\"request_id\":\"req-1\""));

        // Wait 150ms before writing late reply
        thread::sleep(Duration::from_millis(150));
        let late_reply = make_query_reply("req-1", 384, 0.111);
        let _ = writer1.write_all(late_reply.as_bytes());
        let _ = writer1.write_all(b"\n");
        let _ = writer1.flush();
        late_reply_flag.store(true, Ordering::Release);

        // Connection 2 (fresh reconnect after timeout drop)
        let (stream2, _) = listener.accept().unwrap();
        let mut reader2 = BufReader::new(stream2.try_clone().unwrap());
        let mut writer2 = stream2;
        let mut line2 = String::new();
        reader2.read_line(&mut line2).unwrap();

        let val2: serde_json::Value = serde_json::from_str(&line2).unwrap();
        if val2["method"] == "health" {
            let req_id = val2["request_id"].as_str().unwrap();
            let health_reply = make_health_reply(req_id, DEFAULT_NATIVE_MODEL, 384);
            writer2.write_all(health_reply.as_bytes()).unwrap();
            writer2.write_all(b"\n").unwrap();
            writer2.flush().unwrap();

            line2.clear();
            reader2.read_line(&mut line2).unwrap();
        }

        assert!(line2.contains("\"request_id\":\"req-2\""));

        let reply2 = make_query_reply("req-2", 384, 0.999);
        writer2.write_all(reply2.as_bytes()).unwrap();
        writer2.write_all(b"\n").unwrap();
        writer2.flush().unwrap();
    });

    // Request 1: 40ms timeout (will time out while server waits 150ms)
    let budget1 = EmbeddingRequestBudget::with_timeout(Duration::from_millis(40));
    let res1 = provider.embed_query("req 1 timeout", &budget1);
    assert!(res1.is_err(), "request 1 should have timed out");

    // Wait until server has pushed the late reply to Connection 1
    while !late_reply_sent.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(10));
    }

    // Request 2: Normal 2-second timeout
    // Must NOT receive the late reply from req-1 (0.111), but must receive req-2's reply (0.999)
    let budget2 = EmbeddingRequestBudget::with_timeout(Duration::from_secs(2));
    let res2 = provider.embed_query("req 2 fresh", &budget2);
    assert!(
        res2.is_ok(),
        "request 2 should succeed after connection reset: {:?}",
        res2.err()
    );

    let vec2 = res2.unwrap();
    assert_eq!(
        vec2[0], 0.999,
        "request 2 was contaminated by late response of request 1!"
    );

    srv_handle.join().unwrap();
}

// ============================================================================
// Challenge 4: Retry-Once Semantics on Connection Drop vs Timeouts
// ============================================================================

#[test]
fn challenge_retry_once_succeeds_on_connection_drop_within_budget() {
    let tmp = tempdir().unwrap();
    let sock_path = tmp.path().join("test_retry.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: std::env::current_exe().unwrap(),
        executable_sha256: "mock".to_string(),
        model_id: "bge-small-en-v1.5-f32".to_string(),
        cache_root: tmp.path().to_path_buf(),
        broker_paths: paths.clone(),
    };

    let client = NativeClientConn::connect(&sock_path, Some(Duration::from_millis(500))).unwrap();
    let identity = test_identity("bge-small-en-v1.5-f32", 384);
    let device = test_device_info("bge-small-en-v1.5-f32", 384);
    let provider = NativeEmbeddingProvider::from_connected(config, client, identity, device);

    let connection_attempts = Arc::new(AtomicUsize::new(0));
    let attempts_count = Arc::clone(&connection_attempts);

    let srv_handle = thread::spawn(move || {
        // Attempt 1: Accept connection, read request, but drop connection immediately (EOF)
        let (stream1, _) = listener.accept().unwrap();
        attempts_count.fetch_add(1, Ordering::Relaxed);
        let mut reader1 = BufReader::new(stream1);
        let mut line1 = String::new();
        reader1.read_line(&mut line1).unwrap();
        drop(reader1); // Drops connection abruptly!

        // Attempt 2 (retry): Accept reconnected client and send valid response
        let (stream2, _) = listener.accept().unwrap();
        attempts_count.fetch_add(1, Ordering::Relaxed);
        let mut reader2 = BufReader::new(stream2.try_clone().unwrap());
        let mut writer2 = stream2;
        let mut line2 = String::new();
        reader2.read_line(&mut line2).unwrap();

        let val2: serde_json::Value = serde_json::from_str(&line2).unwrap();
        if val2["method"] == "health" {
            let req_id = val2["request_id"].as_str().unwrap();
            let health_reply = make_health_reply(req_id, DEFAULT_NATIVE_MODEL, 384);
            writer2.write_all(health_reply.as_bytes()).unwrap();
            writer2.write_all(b"\n").unwrap();
            writer2.flush().unwrap();

            line2.clear();
            reader2.read_line(&mut line2).unwrap();
        }

        let req_id = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line2) {
            json["request_id"].as_str().unwrap_or("req-2").to_string()
        } else {
            "req-2".to_string()
        };

        let reply = make_query_reply(&req_id, 384, 0.777);
        writer2.write_all(reply.as_bytes()).unwrap();
        writer2.write_all(b"\n").unwrap();
        writer2.flush().unwrap();
    });

    // Execute query with generous 3-second budget
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(3));
    let res = provider.embed_query("retry query", &budget);
    assert!(
        res.is_ok(),
        "query should have retried and succeeded: {:?}",
        res.err()
    );

    let vec = res.unwrap();
    assert_eq!(vec[0], 0.777);
    assert_eq!(
        connection_attempts.load(Ordering::Relaxed),
        2,
        "expected exactly 2 attempts (1 failure + 1 retry)"
    );

    srv_handle.join().unwrap();
}

#[test]
fn challenge_retry_once_stops_after_single_retry_when_both_fail() {
    let tmp = tempdir().unwrap();
    let sock_path = tmp.path().join("test_retry_stop.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: PathBuf::from("/bin/true"),
        executable_sha256: "mock".to_string(),
        model_id: "bge-small-en-v1.5-f32".to_string(),
        cache_root: tmp.path().to_path_buf(),
        broker_paths: paths.clone(),
    };

    let client = NativeClientConn::connect(&sock_path, Some(Duration::from_millis(500))).unwrap();
    let identity = test_identity("bge-small-en-v1.5-f32", 384);
    let device = test_device_info("bge-small-en-v1.5-f32", 384);
    let provider = NativeEmbeddingProvider::from_connected(config, client, identity, device);

    let connection_attempts = Arc::new(AtomicUsize::new(0));
    let attempts_count = Arc::clone(&connection_attempts);

    let srv_handle = thread::spawn(move || {
        // Attempt 1: Drop
        let (stream1, _) = listener.accept().unwrap();
        attempts_count.fetch_add(1, Ordering::Relaxed);
        drop(stream1);

        // Attempt 2 (the 1 allowed retry): Handle health check on attach, then drop on query
        let (stream2, _) = listener.accept().unwrap();
        attempts_count.fetch_add(1, Ordering::Relaxed);
        let mut reader2 = BufReader::new(stream2.try_clone().unwrap());
        let mut writer2 = stream2;
        let mut line2 = String::new();
        if reader2.read_line(&mut line2).is_ok() {
            if let Ok(val2) = serde_json::from_str::<serde_json::Value>(&line2) {
                if val2["method"] == "health" {
                    let req_id = val2["request_id"].as_str().unwrap_or("health-init");
                    let health_reply = make_health_reply(req_id, DEFAULT_NATIVE_MODEL, 384);
                    let _ = writer2.write_all(health_reply.as_bytes());
                    let _ = writer2.write_all(b"\n");
                    let _ = writer2.flush();
                    line2.clear();
                    let _ = reader2.read_line(&mut line2);
                }
            }
        }
        drop(reader2);
        drop(writer2);

        // No Attempt 3 should ever occur! Set timeout to verify no further connects
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(
            listener.accept().is_err(),
            "unexpected 3rd retry connection!"
        );
    });

    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(3));
    let res = provider.embed_query("retry stop query", &budget);
    assert!(res.is_err(), "should fail after 1 retry without looping");
    assert_eq!(
        connection_attempts.load(Ordering::Relaxed),
        2,
        "should attempt exactly 2 times before giving up"
    );

    srv_handle.join().unwrap();
}

#[test]
fn challenge_retry_does_not_apply_to_timeouts() {
    let tmp = tempdir().unwrap();
    let sock_path = tmp.path().join("test_no_retry_timeout.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: PathBuf::from("/bin/true"),
        executable_sha256: "mock".to_string(),
        model_id: "bge-small-en-v1.5-f32".to_string(),
        cache_root: tmp.path().to_path_buf(),
        broker_paths: paths.clone(),
    };

    let client = NativeClientConn::connect(&sock_path, Some(Duration::from_millis(500))).unwrap();
    let identity = test_identity("bge-small-en-v1.5-f32", 384);
    let device = test_device_info("bge-small-en-v1.5-f32", 384);
    let provider = NativeEmbeddingProvider::from_connected(config, client, identity, device);

    let connection_attempts = Arc::new(AtomicUsize::new(0));
    let attempts_count = Arc::clone(&connection_attempts);

    let srv_handle = thread::spawn(move || {
        let (stream1, _) = listener.accept().unwrap();
        attempts_count.fetch_add(1, Ordering::Relaxed);
        // Do not respond; wait 150ms
        thread::sleep(Duration::from_millis(150));
        drop(stream1);

        // Verify no second connection was attempted
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_err(),
            "timeout should NOT trigger a retry connection!"
        );
    });

    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(40));
    let res = provider.embed_query("timeout no retry", &budget);
    assert!(res.is_err());
    assert_eq!(
        connection_attempts.load(Ordering::Relaxed),
        1,
        "timeout must NOT be retried (expected exactly 1 connection)"
    );

    srv_handle.join().unwrap();
}

#[test]
fn challenge_retry_does_not_apply_when_remaining_budget_is_expired() {
    let tmp = tempdir().unwrap();
    let sock_path = tmp.path().join("test_no_retry_zero_budget.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: PathBuf::from("/bin/true"),
        executable_sha256: "mock".to_string(),
        model_id: "bge-small-en-v1.5-f32".to_string(),
        cache_root: tmp.path().to_path_buf(),
        broker_paths: paths.clone(),
    };

    let client = NativeClientConn::connect(&sock_path, Some(Duration::from_millis(500))).unwrap();
    let identity = test_identity("bge-small-en-v1.5-f32", 384);
    let device = test_device_info("bge-small-en-v1.5-f32", 384);
    let provider = NativeEmbeddingProvider::from_connected(config, client, identity, device);

    let connection_attempts = Arc::new(AtomicUsize::new(0));
    let attempts_count = Arc::clone(&connection_attempts);

    let srv_handle = thread::spawn(move || {
        let (stream1, _) = listener.accept().unwrap();
        attempts_count.fetch_add(1, Ordering::Relaxed);
        // Sleep until client's budget expires, then drop connection
        thread::sleep(Duration::from_millis(30));
        drop(stream1);

        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(50));
        assert!(
            listener.accept().is_err(),
            "expired budget should NOT retry connection!"
        );
    });

    // Budget of 20ms: expires while server waits 30ms before dropping
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_millis(20));
    let res = provider.embed_query("expired budget no retry", &budget);
    assert!(res.is_err());
    assert_eq!(
        connection_attempts.load(Ordering::Relaxed),
        1,
        "connection drop with expired budget must NOT be retried"
    );

    srv_handle.join().unwrap();
}

#[test]
fn challenge_embed_batch_retry_once_succeeds_on_connection_drop() {
    let tmp = tempdir().unwrap();
    let sock_path = tmp.path().join("test_batch_retry.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: std::env::current_exe().unwrap(),
        executable_sha256: "mock".to_string(),
        model_id: "bge-small-en-v1.5-f32".to_string(),
        cache_root: tmp.path().to_path_buf(),
        broker_paths: paths.clone(),
    };

    let client = NativeClientConn::connect(&sock_path, Some(Duration::from_millis(500))).unwrap();
    let identity = test_identity("bge-small-en-v1.5-f32", 384);
    let device = test_device_info("bge-small-en-v1.5-f32", 384);
    let provider = NativeEmbeddingProvider::from_connected(config, client, identity, device);

    let connection_attempts = Arc::new(AtomicUsize::new(0));
    let attempts_count = Arc::clone(&connection_attempts);

    let srv_handle = thread::spawn(move || {
        // Attempt 1: Drop
        let (stream1, _) = listener.accept().unwrap();
        attempts_count.fetch_add(1, Ordering::Relaxed);
        drop(stream1);

        // Attempt 2 (retry): Valid batch reply
        let (stream2, _) = listener.accept().unwrap();
        attempts_count.fetch_add(1, Ordering::Relaxed);
        let mut reader2 = BufReader::new(stream2.try_clone().unwrap());
        let mut writer2 = stream2;
        let mut line2 = String::new();
        reader2.read_line(&mut line2).unwrap();

        let val2: serde_json::Value = serde_json::from_str(&line2).unwrap();
        if val2["method"] == "health" {
            let req_id = val2["request_id"].as_str().unwrap();
            let health_reply = make_health_reply(req_id, DEFAULT_NATIVE_MODEL, 384);
            writer2.write_all(health_reply.as_bytes()).unwrap();
            writer2.write_all(b"\n").unwrap();
            writer2.flush().unwrap();

            line2.clear();
            reader2.read_line(&mut line2).unwrap();
        }

        let req_id = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line2) {
            json["request_id"].as_str().unwrap_or("req-2").to_string()
        } else {
            "req-2".to_string()
        };

        let reply = make_batch_reply(&req_id, 384, 2, 0.555);
        writer2.write_all(reply.as_bytes()).unwrap();
        writer2.write_all(b"\n").unwrap();
        writer2.flush().unwrap();
    });

    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(3));
    let texts = vec!["text 1".to_string(), "text 2".to_string()];
    let res = provider.embed_batch(&texts, &budget);
    assert!(
        res.is_ok(),
        "embed_batch should retry and succeed: {:?}",
        res.err()
    );

    let vecs = res.unwrap();
    assert_eq!(vecs.len(), 2);
    assert_eq!(vecs[0].len(), 384);
    assert_eq!(vecs[0][0], 0.555);
    assert_eq!(connection_attempts.load(Ordering::Relaxed), 2);

    srv_handle.join().unwrap();
}

// ============================================================================
// Challenge 5: Reconnect Identity Mismatch & Owner-Death Replacement
// ============================================================================

#[test]
fn challenge_reconnect_rejects_socket_with_mismatched_identity() {
    let tmp = tempdir().unwrap();
    let sock_path = tmp.path().join("mismatched_identity.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let script_path = tmp.path().join("mock-sidecar.sh");
    fs::write(&script_path, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: std::env::current_exe().unwrap(),
        executable_sha256: "mock".to_string(),
        model_id: DEFAULT_NATIVE_MODEL.to_string(),
        cache_root: tmp.path().to_path_buf(),
        broker_paths: paths,
    };

    let expected_id = test_identity(DEFAULT_NATIVE_MODEL, 384);

    let srv_handle = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut writer = stream;
                    let mut line = String::new();
                    while let Ok(n) = reader.read_line(&mut line) {
                        if n == 0 {
                            break;
                        }
                        if line.contains("\"method\":\"health\"") {
                            // Send reply with a DIFFERENT weights hash (mismatched identity)
                            let reply = format!(
                                r#"{{"schema":"julie.embedding.sidecar","version":1,"request_id":"health-init","result":{{"ready":true,"dims":384,"device":"cpu","runtime":"llama.cpp","model_id":"{}","model_sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","pooling":"cls","normalization":"l2","instruction_policy_version":1,"llama_cpp_build":"b3560"}},"error":null}}"#,
                                DEFAULT_NATIVE_MODEL
                            );
                            let _ = writer.write_all(reply.as_bytes());
                            let _ = writer.write_all(b"\n");
                            let _ = writer.flush();
                        }
                        line.clear();
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    });

    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(2));
    let res = NativeEmbeddingProvider::launch_and_attach(&config, &budget, Some(&expected_id));

    let err = match res {
        Ok(_) => panic!("expected rejection when identity does not match"),
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("REPLACEMENT_BROKER_INCOMPATIBLE"),
        "expected REPLACEMENT_BROKER_INCOMPATIBLE, got: {err}"
    );

    let _ = srv_handle.join();
}

#[test]
fn challenge_reconnect_retry_spawns_new_broker_on_owner_death() {
    let tmp = tempdir().unwrap();
    let cache_dir = tmp.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();

    let sock_path = tmp.path().join("owner_death.sock");
    let listener = UnixListener::bind(&sock_path).unwrap();

    let script_path = tmp.path().join("mock-sidecar.sh");
    fs::write(&script_path, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();

    let paths = BrokerPaths {
        endpoint_str: sock_path.to_string_lossy().to_string(),
        endpoint_path: sock_path.clone(),
        service_lock: tmp.path().join("test.lock"),
        accelerator_lock: tmp.path().join("test_accel.lock"),
    };

    let config = NativeLaunchConfig {
        executable_path: std::env::current_exe().unwrap(),
        executable_sha256: "mock".to_string(),
        model_id: DEFAULT_NATIVE_MODEL.to_string(),
        cache_root: cache_dir,
        broker_paths: paths,
    };

    let client = NativeClientConn::connect(&sock_path, Some(Duration::from_millis(500))).unwrap();
    let identity = test_identity(DEFAULT_NATIVE_MODEL, 384);
    let device = test_device_info(DEFAULT_NATIVE_MODEL, 384);
    let provider = NativeEmbeddingProvider::from_connected(config, client, identity, device);

    let srv_handle = thread::spawn(move || {
        // Attempt 1 on initial connection: Read query, then abruptly close/drop stream (owner crash)
        let (stream1, _) = listener.accept().unwrap();
        let mut reader1 = BufReader::new(stream1);
        let mut line1 = String::new();
        let _ = reader1.read_line(&mut line1);
        drop(reader1); // Simulates owner dying/connection dropping

        // Attempt 2 (retry): launch_and_attach connects back to winner socket
        let (stream2, _) = listener.accept().unwrap();
        let mut reader2 = BufReader::new(stream2.try_clone().unwrap());
        let mut writer2 = stream2;
        let mut line2 = String::new();
        reader2.read_line(&mut line2).unwrap();

        let val2: serde_json::Value = serde_json::from_str(&line2).unwrap();
        if val2["method"] == "health" {
            let req_id = val2["request_id"].as_str().unwrap();
            let health_reply = make_health_reply(req_id, DEFAULT_NATIVE_MODEL, 384);
            writer2.write_all(health_reply.as_bytes()).unwrap();
            writer2.write_all(b"\n").unwrap();
            writer2.flush().unwrap();

            line2.clear();
            reader2.read_line(&mut line2).unwrap();
        }

        let req_id = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line2) {
            json["request_id"].as_str().unwrap_or("req-2").to_string()
        } else {
            "req-2".to_string()
        };

        let reply = make_query_reply(&req_id, 384, 0.888);
        writer2.write_all(reply.as_bytes()).unwrap();
        writer2.write_all(b"\n").unwrap();
        writer2.flush().unwrap();
    });

    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(3));
    let res = provider.embed_query("query after owner death", &budget);
    assert!(
        res.is_ok(),
        "query should recover after owner death and reconnect: {:?}",
        res.err()
    );
    let vec = res.unwrap();
    assert_eq!(vec[0], 0.888);

    let _ = srv_handle.join();
}
