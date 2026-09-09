//! Adversarial challenge test verifying running executable SHA binding to process
//! across binary replacement and broker recovery.

#![cfg(unix)]

use std::fs;
use std::time::Duration;

use julie_core::embeddings_contract::{EmbeddingProvider, EmbeddingRequestBudget};
use tempfile::tempdir;

use crate::embeddings::factory::EmbeddingConfig;
use crate::embeddings::native::launch::derive_broker_paths;
use crate::embeddings::native::{DEFAULT_NATIVE_MODEL, NativeEmbeddingProvider};

#[test]
#[serial_test::serial(embedding_env)]
fn challenge_replacement_between_hash_and_spawn_is_detected_and_rejected() {
    let dir = tempdir().unwrap();
    let bin_a = dir.path().join("bin_a");
    let bin_b = dir.path().join("bin_b");
    let link_path = dir.path().join("sidecar-link");

    let src_a = dir.path().join("src_a.rs");
    fs::write(
        &src_a,
        "fn main() { std::thread::sleep(std::time::Duration::from_secs(10)); }",
    )
    .unwrap();
    let status_a = std::process::Command::new("rustc")
        .args([src_a.to_str().unwrap(), "-o", bin_a.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status_a.success());

    let src_b = dir.path().join("src_b.rs");
    fs::write(
        &src_b,
        "fn main() { /* binary b */ std::thread::sleep(std::time::Duration::from_secs(10)); }",
    )
    .unwrap();
    let status_b = std::process::Command::new("rustc")
        .args([src_b.to_str().unwrap(), "-o", bin_b.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status_b.success());

    // 1. Initial symlink points to Binary A
    std::os::unix::fs::symlink(&bin_a, &link_path).unwrap();
    let (_, sha_a) =
        crate::embeddings::native::launch::find_and_hash_sidecar_binary(Some(&link_path)).unwrap();

    // 2. Race condition before spawn: symlink is switched to Binary B
    fs::remove_file(&link_path).unwrap();
    std::os::unix::fs::symlink(&bin_b, &link_path).unwrap();

    // 3. Process is spawned from the link, actually executing Binary B
    let mut child = std::process::Command::new(&link_path).spawn().unwrap();

    // 4. Immediately switch symlink back to Binary A on disk
    fs::remove_file(&link_path).unwrap();
    std::os::unix::fs::symlink(&bin_a, &link_path).unwrap();

    // 5. Verification must inspect the actually running process image (/proc/{pid}/exe)
    // and fail closed even though the disk link now points back to Binary A!
    let res = crate::embeddings::native::launch::verify_launched_child_sha(
        &link_path,
        child.id(),
        &sha_a,
    );

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        res.is_err(),
        "verify_launched_child_sha must fail when running process was spawned from replaced binary"
    );
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("LAUNCHED_EXECUTABLE_SHA_MISMATCH"),
        "Error message must indicate LAUNCHED_EXECUTABLE_SHA_MISMATCH, got: {err}"
    );
}

#[test]
#[serial_test::serial(embedding_env)]
fn challenge_native_broker_actual_spawn_and_recovery_updates_running_sha() {
    let dir = tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();

    let bin_path = dir.path().join("mock-broker-bin");
    let src_path = dir.path().join("mock_broker.rs");

    let generate_source = |comment: &str| {
        format!(
            r#"
use std::env;
use std::io::{{BufRead, BufReader, Write}};
use std::os::unix::net::UnixListener;
use std::path::Path;

fn main() {{
    let args: Vec<String> = env::args().collect();
    if args.len() == 99999 {{
        eprintln!("{comment}");
    }}
    let mut endpoint = None;
    let mut i = 1;
    while i < args.len() {{
        if args[i] == "--endpoint" && i + 1 < args.len() {{
            endpoint = Some(args[i + 1].clone());
            i += 1;
        }}
        i += 1;
    }}
    let endpoint = endpoint.expect("missing --endpoint");
    let ep_path = Path::new(&endpoint);
    if ep_path.exists() {{
        let _ = std::fs::remove_file(ep_path);
    }}
    let listener = UnixListener::bind(ep_path).expect("bind failed");
    for stream in listener.incoming() {{
        let mut stream = stream.expect("accept failed");
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {{
            if line.trim().is_empty() {{
                line.clear();
                continue;
            }}
            let req_id = if let Some(pos) = line.find("\"request_id\":") {{
                let rest = &line[pos + 13..];
                if let Some(start) = rest.find('"') {{
                    if let Some(end) = rest[start + 1..].find('"') {{
                        &rest[start + 1..start + 1 + end]
                    }} else {{
                        "req-1"
                    }}
                }} else {{
                    "req-1"
                }}
            }} else {{
                "req-1"
            }};

            let mut reply = String::from("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"");
            reply.push_str(req_id);
            if line.contains("\"health\"") {{
                reply.push_str("\",\"result\":{{\"ready\":true,\"dims\":384,\"device\":\"cpu\",\"runtime\":\"llama.cpp\",\"model_id\":\"bge-small-en-v1.5-f32\",\"model_sha256\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\",\"pooling\":\"cls\",\"normalization\":\"l2\",\"instruction_policy_version\":1,\"llama_cpp_build\":\"b3560\"}},\"error\":null}}\n");
            }} else if line.contains("\"embed_query\"") {{
                reply.push_str("\",\"result\":{{\"dims\":384,\"vector\":[");
                for idx in 0..384 {{
                    if idx > 0 {{ reply.push(','); }}
                    reply.push_str("0.42");
                }}
                reply.push_str("]}},\"error\":null}}\n");
            }} else {{
                reply.push_str("\",\"result\":null,\"error\":null}}\n");
            }}
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
            line.clear();
        }}
    }}
}}
"#
        )
    };

    // Compile Version 1 of the mock broker
    fs::write(&src_path, generate_source("Version 1")).unwrap();
    let status1 = std::process::Command::new("rustc")
        .args([src_path.to_str().unwrap(), "-o", bin_path.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status1.success());

    let (_, sha1) =
        crate::embeddings::native::launch::find_and_hash_sidecar_binary(Some(&bin_path)).unwrap();

    let config = EmbeddingConfig {
        provider: "native".to_string(),
        cache_dir: Some(cache_dir.clone()),
        native_program: Some(bin_path.clone()),
        native_model: Some(DEFAULT_NATIVE_MODEL.to_string()),
    };

    // This actually spawns the mock broker ELF binary process via spawn_broker!
    let provider = NativeEmbeddingProvider::try_new(&config).unwrap();
    assert_eq!(
        provider.running_executable_sha(),
        Some(sha1.clone()),
        "Initially spawned broker must report sha1"
    );

    // Embed query succeeds on version 1 broker
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(5));
    let v1 = provider.embed_query("test-v1", &budget).unwrap();
    assert_eq!(v1.len(), 384);

    // Replace binary on disk with Version 2
    let bin_path_v2 = dir.path().join("mock-broker-bin-v2");
    fs::write(&src_path, generate_source("Version 2 with new bytes")).unwrap();
    let status2 = std::process::Command::new("rustc")
        .args([
            src_path.to_str().unwrap(),
            "-o",
            bin_path_v2.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status2.success());

    fs::rename(&bin_path_v2, &bin_path).unwrap();
    let (_, sha2) =
        crate::embeddings::native::launch::find_and_hash_sidecar_binary(Some(&bin_path)).unwrap();
    assert_ne!(sha1, sha2, "Replaced binary must have different SHA");

    // Terminate the connection handle to force broker relaunch/reconnect
    provider.shutdown();

    // Query on provider now triggers relaunch of the replaced broker!
    let v2 = provider.embed_query("test-v2", &budget).unwrap();
    assert_eq!(v2.len(), 384);

    // Verification: running_executable_sha now reports sha2 of the actually spawned replacement!
    assert_eq!(
        provider.running_executable_sha(),
        Some(sha2),
        "Running executable SHA must reflect the replaced binary after actual spawn and recovery"
    );

    provider.shutdown();
}

#[test]
#[serial_test::serial(embedding_env)]
fn challenge_broker_spawn_race_loser_exits_zero_and_connects_to_winner() {
    let dir = tempdir().unwrap();
    let cache_dir = dir.path().join("cache");
    fs::create_dir_all(&cache_dir).unwrap();

    let src_path = dir.path().join("mock_broker.rs");
    let bin_path = dir.path().join("mock-broker-bin");

    // Rust source for mock broker:
    // If MOCK_WINNER is set:
    //   waits for loser child to be spawned (signaled by MOCK_SPAWNED_MARKER file),
    //   then binds the endpoint and answers health and embed_query requests.
    // If MOCK_WINNER is not set (the spawned child):
    //   writes MOCK_SPAWNED_MARKER file and exits 0 immediately (guaranteeing loser in acquisition race).
    let src_code = r#"
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::thread;
use std::time::Duration;

fn main() {
    if env::var("MOCK_WINNER").is_ok() {
        let endpoint = env::var("MOCK_ENDPOINT").expect("MOCK_ENDPOINT required");
        let ep_path = Path::new(&endpoint);
        let marker = env::var("MOCK_SPAWNED_MARKER").expect("MOCK_SPAWNED_MARKER required");
        let marker_path = Path::new(&marker);
        // Wait until loser child is actually spawned and wrote the marker
        let mut waited = 0;
        while !marker_path.exists() && waited < 500 {
            thread::sleep(Duration::from_millis(10));
            waited += 1;
        }
        let listener = UnixListener::bind(ep_path).expect("bind failed");
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
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
                            } else {
                                "req-1"
                            }
                        } else {
                            "req-1"
                        }
                    } else {
                        "req-1"
                    };

                    let mut reply = String::from("{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"");
                    reply.push_str(req_id);
                    if line.contains("\"health\"") {
                        reply.push_str("\",\"result\":{\"ready\":true,\"dims\":384,\"device\":\"cpu\",\"runtime\":\"llama.cpp\",\"model_id\":\"bge-small-en-v1.5-f32\",\"model_sha256\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\",\"pooling\":\"cls\",\"normalization\":\"l2\",\"instruction_policy_version\":1,\"llama_cpp_build\":\"b3560\"},\"error\":null}\n");
                    } else if line.contains("\"embed_query\"") {
                        reply.push_str("\",\"result\":{\"dims\":384,\"vector\":[");
                        for idx in 0..384 {
                            if idx > 0 { reply.push(','); }
                            reply.push_str("0.42");
                        }
                        reply.push_str("]},\"error\":null}\n");
                    } else {
                        reply.push_str("\",\"result\":null,\"error\":null}\n");
                    }
                    let _ = stream.write_all(reply.as_bytes());
                    let _ = stream.flush();
                    line.clear();
                }
            }
        }
    } else {
        // Loser child: signal sync marker that child is exiting, then exit 0 immediately
        if let Ok(marker) = env::var("JULIE_TEST_SYNC_BEFORE_SPAWN_VERIFY") {
            let _ = std::fs::write(&marker, "1");
        }
        std::process::exit(0);
    }
}
"#;

    fs::write(&src_path, src_code).unwrap();
    let status = std::process::Command::new("rustc")
        .args([src_path.to_str().unwrap(), "-o", bin_path.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success(), "rustc compilation failed");

    let (_, sha) =
        crate::embeddings::native::launch::find_and_hash_sidecar_binary(Some(&bin_path)).unwrap();
    let broker_paths = derive_broker_paths(&cache_dir, &sha, DEFAULT_NATIVE_MODEL).unwrap();
    let endpoint = broker_paths.endpoint_path.clone();

    let sync_marker = dir.path().join("loser_exited.marker");
    let error_branch_marker = dir.path().join("error_branch.marker");
    struct EnvGuard;
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("MOCK_SPAWNED_MARKER");
                std::env::remove_var("JULIE_TEST_SYNC_BEFORE_SPAWN_VERIFY");
                std::env::remove_var("JULIE_TEST_RECORD_ERROR_BRANCH");
            }
        }
    }
    let _env_guard = EnvGuard;
    unsafe {
        std::env::set_var("MOCK_SPAWNED_MARKER", sync_marker.to_str().unwrap());
        std::env::set_var(
            "JULIE_TEST_SYNC_BEFORE_SPAWN_VERIFY",
            sync_marker.to_str().unwrap(),
        );
        std::env::set_var(
            "JULIE_TEST_RECORD_ERROR_BRANCH",
            error_branch_marker.to_str().unwrap(),
        );
    }

    // Spawn the winner process in the background with MOCK_WINNER=1
    let mut winner_child = std::process::Command::new(&bin_path)
        .env("MOCK_WINNER", "1")
        .env("MOCK_ENDPOINT", endpoint.to_str().unwrap())
        .env("MOCK_SPAWNED_MARKER", sync_marker.to_str().unwrap())
        .spawn()
        .expect("failed to spawn winner process");

    let config = EmbeddingConfig {
        provider: "native".to_string(),
        cache_dir: Some(cache_dir.clone()),
        native_program: Some(bin_path.clone()),
        native_model: Some(DEFAULT_NATIVE_MODEL.to_string()),
    };

    // Losing process calls try_new:
    // 1. Initial probe to endpoint fails (winner waits for sync_marker).
    // 2. try_new spawns child (which writes sync_marker and exits 0 immediately).
    // 3. Parent waits for sync_marker + kernel process termination.
    // 4. verify_launched_child_sha fails on dead process, entering spawn/hash-error branch.
    // 5. Loser detection sees child exited 0, records error branch marker, connects to winner!
    let provider_res = NativeEmbeddingProvider::try_new(&config);
    assert!(
        provider_res.is_ok(),
        "loser in spawn race failed to recover and connect to winner: {:?}",
        provider_res.err()
    );
    assert!(
        error_branch_marker.exists(),
        "spawn/hash-error branch was guaranteed to execute"
    );
    assert_eq!(
        fs::read_to_string(&error_branch_marker).unwrap(),
        "spawn_hash_error_branch_executed"
    );
    unsafe {
        std::env::remove_var("MOCK_SPAWNED_MARKER");
        std::env::remove_var("JULIE_TEST_SYNC_BEFORE_SPAWN_VERIFY");
        std::env::remove_var("JULIE_TEST_RECORD_ERROR_BRANCH");
    }

    let provider = provider_res.unwrap();

    // Verify running_executable_sha matches the winner's binary
    assert_eq!(
        provider.running_executable_sha(),
        Some(sha),
        "Winner running executable SHA must be verified"
    );

    // Verify loser can query the winner
    let budget = EmbeddingRequestBudget::with_timeout(Duration::from_secs(5));
    let embed_res = provider.embed_query("test", &budget);
    assert!(
        embed_res.is_ok(),
        "query to winner failed: {:?}",
        embed_res.err()
    );

    // Clean up winner process
    let _ = winner_child.kill();
    let _ = winner_child.wait();
}
