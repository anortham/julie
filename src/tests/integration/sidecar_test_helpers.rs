//! Shared helpers for sidecar-backed integration tests.

#[cfg(feature = "embeddings-sidecar")]
use std::process::{Command, Stdio};

#[cfg(feature = "embeddings-sidecar")]
use crate::embeddings::SidecarEmbeddingProvider;

#[cfg(feature = "embeddings-sidecar")]
pub fn test_python_interpreter() -> String {
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

#[cfg(feature = "embeddings-sidecar")]
pub fn create_test_sidecar_provider() -> SidecarEmbeddingProvider {
    let temp_dir = tempfile::tempdir().expect("temp dir for fake sidecar");
    let script_path = temp_dir.path().join("fake_sidecar.py");
    std::fs::write(
        &script_path,
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
        result = {"ready": True, "runtime": "fake-sidecar", "device": "cpu", "dims": 384}
    elif method == "embed_query":
        result = {"dims": 384, "vector": [0.1] * 384}
    elif method == "embed_batch":
        texts = req.get("params", {}).get("texts", [])
        result = {"dims": 384, "vectors": [[0.2] * 384 for _ in texts]}
    elif method == "shutdown":
        result = {"stopping": True}
    else:
        result = {"dims": 384, "vector": [0.0] * 384}

    response = {
        "schema": "julie.embedding.sidecar",
        "version": 1,
        "request_id": req_id,
        "result": result,
    }
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()

    if method == "shutdown":
        break
"#,
    )
    .expect("fake sidecar script should be written");

    let provider = SidecarEmbeddingProvider::try_new_for_command(
        test_python_interpreter(),
        vec![script_path.to_string_lossy().to_string()],
    )
    .expect("sidecar provider should initialize from fake script");

    std::mem::forget(temp_dir);
    provider
}
