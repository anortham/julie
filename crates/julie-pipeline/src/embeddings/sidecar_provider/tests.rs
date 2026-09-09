use std::process::{Command, Stdio};
use std::time::Duration;

use super::SidecarEmbeddingProvider;
use crate::embeddings::{EmbeddingProvider, EmbeddingRequestBudget};

fn test_python_interpreter() -> String {
    if let Ok(v) = std::env::var("JULIE_TEST_PYTHON") {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return v;
        }
    }
    let candidates = if cfg!(target_os = "windows") {
        vec!["python", "py", "python3"]
    } else {
        vec!["python3", "python"]
    };
    for candidate in candidates {
        let ok = Command::new(candidate)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return candidate.to_string();
        }
    }
    panic!("No Python interpreter found; set JULIE_TEST_PYTHON");
}

/// Verify the circuit breaker: after FATAL_THRESHOLD consecutive fatal
/// failures the provider reports "permanently disabled" instead of
/// attempting yet another respawn.
///
/// The fake sidecar passes the health check (so construction succeeds)
/// but immediately exits on any embed request, triggering a fatal error
/// each time. Without the circuit breaker the provider would respawn
/// indefinitely; with it the 3rd failure permanently disables the
/// provider and subsequent calls fail fast without spawning new processes.
#[test]
fn test_circuit_breaker_permanently_disables_after_consecutive_fatal_failures() {
    // Python sidecar: passes health, crashes immediately on embed.
    let script = r#"
import json, sys
while True:
    line = sys.stdin.readline()
    if not line:
        break
    req = json.loads(line)
    method = req.get("method", "")
    req_id = req.get("request_id", "")
    if method == "health":
        resp = {"schema": "julie.embedding.sidecar", "version": 1,
                "request_id": req_id,
                "result": {"ready": True, "runtime": "crash-sidecar",
                           "device": "cpu", "dims": 4}}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
    elif method.startswith("embed"):
        # Crash without sending a response -- marks connection as fatal.
        sys.exit(1)
    elif method == "shutdown":
        resp = {"schema": "julie.embedding.sidecar", "version": 1,
                "request_id": req_id, "result": {"stopping": True}}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
        break
"#;

    let provider = SidecarEmbeddingProvider::try_new_for_command_with_timeout(
        test_python_interpreter(),
        vec!["-u".to_string(), "-c".to_string(), script.to_string()],
        Duration::from_secs(5),
    )
    .expect("provider construction should succeed (health check passes)");

    // Calls 1 and 2: fatal errors, each triggers a successful respawn.
    // The "permanently disabled" message should NOT appear yet.
    for i in 1..=2 {
        let err = provider
            .embed_query("x", &EmbeddingRequestBudget::default())
            .expect_err("embed should fail due to crash sidecar");
        assert!(
            !err.to_string().contains("permanently disabled"),
            "failure #{i} should not yet trigger circuit breaker, got: {err}"
        );
    }

    // Call 3: fatal error + circuit breaker fires -- permanently disabled.
    let err3 = provider
        .embed_query("x", &EmbeddingRequestBudget::default())
        .expect_err("3rd embed should fail with circuit breaker");
    assert!(
        err3.to_string().contains("permanently disabled"),
        "3rd consecutive failure should trigger circuit breaker, got: {err3}"
    );

    // Call 4: provider is now permanently disabled, should fail fast.
    let err4 = provider
        .embed_query("x", &EmbeddingRequestBudget::default())
        .expect_err("4th embed should fail immediately (permanently disabled)");
    assert!(
        err4.to_string().contains("permanently disabled"),
        "4th call should still be permanently disabled, got: {err4}"
    );
}
