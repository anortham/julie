//! Tests for the Rust sidecar embedding provider client.

#[cfg(test)]
#[cfg(feature = "embeddings-sidecar")]
mod tests {
    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use tempfile::TempDir;

    use crate::embeddings::{EmbeddingProvider, SidecarEmbeddingProvider};

    fn test_python_interpreter() -> String {
        if let Ok(override_value) = std::env::var("JULIE_TEST_PYTHON") {
            let trimmed = override_value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        let candidates = if cfg!(target_os = "windows") {
            vec!["python", "py", "python3"]
        } else {
            vec!["python3", "python"]
        };

        for candidate in candidates {
            let available = Command::new(candidate)
                .arg("--version")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success());
            if available {
                return candidate.to_string();
            }
        }

        panic!("No Python interpreter found for tests; set JULIE_TEST_PYTHON");
    }

    fn build_test_sidecar_provider(mode: &str, dims: usize, marker: Option<&str>) -> SidecarEmbeddingProvider {
        let script = r#"import json
import sys

mode = sys.argv[1]
dims = int(sys.argv[2])
marker = sys.argv[3] if len(sys.argv) > 3 else ""

while True:
    line = sys.stdin.readline()
    if not line:
        break
    req = json.loads(line)
    method = req.get("method")
    req_id = req.get("request_id", "")

    if method == "shutdown":
        if marker:
            with open(marker, "w", encoding="utf-8") as f:
                f.write("shutdown")
        resp = {
            "schema": "julie.embedding.sidecar",
            "version": 1,
            "request_id": req_id,
            "result": {"stopping": True},
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
        break

    if method == "health":
        ready = mode != "bad_health"
        result = {"ready": ready, "runtime": "fake-sidecar", "device": "cpu", "dims": dims}
        resp = {
            "schema": "julie.embedding.sidecar",
            "version": 1,
            "request_id": req_id,
            "result": result,
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
        continue

    if mode == "error_envelope" and method == "embed_query":
        resp = {
            "schema": "julie.embedding.sidecar",
            "version": 1,
            "request_id": req_id,
            "error": {"code": "boom", "message": "query failed"},
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
        continue

    response_dims = 256 if mode == "bad_dims_response" else dims

    if method == "embed_query":
        result = {"dims": response_dims, "vector": [0.5] * response_dims}
    elif method == "embed_batch":
        texts = req.get("params", {}).get("texts", [])
        result = {"dims": response_dims, "vectors": [[0.25] * response_dims for _ in texts]}
    else:
        result = {"dims": response_dims, "vector": [0.0] * response_dims}

    response_id = "wrong-id" if mode == "request_id_mismatch" else req_id
    resp = {
        "schema": "julie.embedding.sidecar",
        "version": 1,
        "request_id": response_id,
        "result": result,
    }
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
"#;

        SidecarEmbeddingProvider::try_new_for_command(
            test_python_interpreter(),
            vec![
                "-u".to_string(),
                "-c".to_string(),
                script.to_string(),
                mode.to_string(),
                dims.to_string(),
                marker.unwrap_or("").to_string(),
            ],
        )
        .expect("test sidecar provider should initialize")
    }

    #[tokio::test]
    async fn test_sidecar_provider_embed_batch_roundtrip() {
        let provider = build_test_sidecar_provider("normal", 384, None);
        let out = provider
            .embed_batch(&["a".to_string(), "b".to_string()])
            .expect("embed_batch should succeed");

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].len(), 384);
        assert_eq!(out[1].len(), 384);
    }

    #[tokio::test]
    async fn test_sidecar_provider_rejects_bad_dimensions() {
        let provider = build_test_sidecar_provider("bad_dims_response", 384, None);
        let err = provider
            .embed_query("x")
            .expect_err("query response with bad dimensions must be rejected");

        assert!(
            err.to_string().contains("384"),
            "expected error to mention 384-d contract, got: {err}"
        );
    }

    #[test]
    fn test_sidecar_provider_surfaces_error_envelope_as_anyhow_error() {
        let provider = build_test_sidecar_provider("error_envelope", 384, None);
        let err = provider
            .embed_query("x")
            .expect_err("error envelope should be surfaced as provider error");

        let msg = err.to_string();
        assert!(
            msg.contains("sidecar error") && msg.contains("boom") && msg.contains("query failed"),
            "expected sidecar error envelope details in anyhow error, got: {msg}"
        );
    }

    #[test]
    fn test_sidecar_provider_rejects_request_id_mismatch() {
        let provider = build_test_sidecar_provider("request_id_mismatch", 384, None);
        let err = provider
            .embed_query("x")
            .expect_err("request id mismatch must be rejected");

        assert!(
            err.to_string().contains("request_id mismatch"),
            "expected request_id mismatch error, got: {err}"
        );
    }

    #[test]
    fn test_sidecar_provider_concurrent_calls_are_serialized_safely() {
        let provider = Arc::new(build_test_sidecar_provider("normal", 384, None));
        let mut joins = Vec::new();

        for i in 0..8 {
            let provider = Arc::clone(&provider);
            joins.push(thread::spawn(move || {
                let text = format!("q-{i}");
                let embedding = provider
                    .embed_query(&text)
                    .expect("concurrent embed_query should succeed");
                assert_eq!(embedding.len(), 384);
            }));
        }

        for join in joins {
            join.join().expect("thread should complete without panic");
        }
    }

    #[test]
    fn test_sidecar_provider_drop_requests_shutdown() {
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let marker = temp_dir.path().join("shutdown.marker");
        let marker_str = marker.to_string_lossy().to_string();

        {
            let provider = build_test_sidecar_provider("normal", 384, Some(&marker_str));
            let _ = provider
                .embed_query("x")
                .expect("provider should respond before drop");
        }

        let mut found = false;
        for _ in 0..20 {
            if marker.exists() {
                found = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }

        assert!(found, "expected shutdown marker file after provider drop");
    }

    #[test]
    fn test_sidecar_provider_constructor_fails_when_health_not_ready() {
        let result = SidecarEmbeddingProvider::try_new_for_command(
            test_python_interpreter(),
            vec![
                "-u".to_string(),
                "-c".to_string(),
                r#"import json
import sys
while True:
    line = sys.stdin.readline()
    if not line:
        break
    req = json.loads(line)
    req_id = req.get("request_id", "")
    method = req.get("method")
    if method == "health":
        resp = {
            "schema": "julie.embedding.sidecar",
            "version": 1,
            "request_id": req_id,
            "result": {"ready": False, "dims": 384},
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
    elif method == "shutdown":
        resp = {
            "schema": "julie.embedding.sidecar",
            "version": 1,
            "request_id": req_id,
            "result": {"stopping": True},
        }
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
        break
"#
                .to_string(),
            ],
        );
        let err = match result {
            Ok(_) => panic!("provider construction should fail when health.ready=false"),
            Err(err) => err,
        };

        assert!(
            err.to_string().contains("not ready"),
            "expected readiness probe failure, got: {err}"
        );
    }
}
