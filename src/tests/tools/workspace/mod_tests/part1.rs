// Tests for `workspace::JulieWorkspace` extracted from the implementation module.

#[cfg(feature = "embeddings-sidecar")]
use crate::daemon::embedding_service::EmbeddingService;
use crate::embeddings::{DeviceInfo, EmbeddingBackend, EmbeddingProvider, EmbeddingRuntimeStatus};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::startup::run_primary_workspace_repair;
use crate::tools::workspace::ManageWorkspaceTool;
#[cfg(feature = "embeddings-sidecar")]
use crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding;
use crate::tools::workspace::indexing::engine_version::{
    SEMANTIC_INDEX_ENGINE_COMPONENT, SEMANTIC_INDEX_ENGINE_VERSION,
};
use crate::workspace::JulieWorkspace;
use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, NumberOrString, ServerJsonRpcMessage, ServerRequest},
    service::{RequestContext, serve_directly},
};
#[cfg(feature = "embeddings-sidecar")]
use serial_test::serial;
#[cfg(feature = "embeddings-sidecar")]
use std::collections::HashMap;
#[cfg(feature = "embeddings-sidecar")]
use std::ffi::OsString;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, atomic::AtomicUsize};
#[cfg(feature = "embeddings-sidecar")]
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct NoopEmbeddingProvider;

impl EmbeddingProvider for NoopEmbeddingProvider {
    fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.1_f32; 384])
    }

    fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1_f32; 384]).collect())
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "pytorch-sidecar".to_string(),
            device: "cpu".to_string(),
            model_name: "noop".to_string(),
            dimensions: 384,
        }
    }
}

#[derive(Default)]
struct BatchMarkerEmbeddingProvider {
    calls: AtomicUsize,
}

impl EmbeddingProvider for BatchMarkerEmbeddingProvider {
    fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.0_f32; 384])
    }

    fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        let marker = (self.calls.fetch_add(1, Ordering::SeqCst) + 1) as f32;
        Ok(texts
            .iter()
            .map(|_| {
                let mut vector = vec![0.0_f32; 384];
                vector[0] = marker;
                vector
            })
            .collect())
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "pytorch-sidecar".to_string(),
            device: "cpu".to_string(),
            model_name: "batch-marker".to_string(),
            dimensions: 384,
        }
    }
}

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn wait_for_embedding_tasks_to_finish(handler: &JulieServerHandler) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let tasks = handler.embedding_tasks.lock().await;
        if tasks.is_empty() {
            break;
        }
        drop(tasks);
        assert!(
            Instant::now() < deadline,
            "Embedding task did not complete within 5s"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn embedding_count_for_primary(handler: &JulieServerHandler) -> i64 {
    let workspace = handler
        .get_workspace()
        .await
        .unwrap()
        .expect("workspace should be initialized");
    let db = workspace.db.as_ref().expect("workspace db should exist");
    db.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .embedding_count()
        .unwrap()
}

async fn first_embedding_value_for_symbol(handler: &JulieServerHandler, name: &str) -> f32 {
    let workspace = handler
        .get_workspace()
        .await
        .unwrap()
        .expect("workspace should be initialized");
    let db = workspace.db.as_ref().expect("workspace db should exist");
    let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let symbol = db
        .find_symbols_by_name(name)
        .unwrap()
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("symbol {name} should exist"));
    db.get_embedding(&symbol.id)
        .unwrap()
        .unwrap_or_else(|| panic!("symbol {name} should have an embedding"))
        .first()
        .copied()
        .expect("embedding should not be empty")
}

async fn send_json_line(writer: &mut (impl AsyncWriteExt + Unpin), value: &serde_json::Value) {
    writer
        .write_all(serde_json::to_string(value).unwrap().as_bytes())
        .await
        .unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();
}

async fn read_server_message(
    lines: &mut tokio::io::Lines<BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>>,
) -> ServerJsonRpcMessage {
    let line = lines
        .next_line()
        .await
        .unwrap()
        .expect("server should emit a JSON-RPC message line");
    serde_json::from_str(&line).unwrap()
}

#[cfg(feature = "embeddings-sidecar")]
use crate::tests::integration::sidecar_test_helpers::test_python_interpreter;

#[cfg(feature = "embeddings-sidecar")]
fn write_slow_health_sidecar_script(
    temp_dir: &TempDir,
    marker_path: &std::path::Path,
) -> std::path::PathBuf {
    let sidecar_script = temp_dir.path().join("slow_health_sidecar.py");
    let marker_literal = marker_path.to_string_lossy();
    std::fs::write(
        &sidecar_script,
        format!(
            r#"import json
import pathlib
import sys
import time

MARKER_PATH = pathlib.Path({marker_literal:?})

while True:
    line = sys.stdin.readline()
    if not line:
        break
    req = json.loads(line)
    req_id = req.get("request_id", "")
    method = req.get("method")

    if method == "health":
        MARKER_PATH.write_text("health-started", encoding="utf-8")
        time.sleep(0.35)
        result = {{"ready": True, "runtime": "slow-fake-sidecar", "device": "cpu", "dims": 384}}
    elif method == "embed_query":
        result = {{"dims": 384, "vector": [0.1] * 384}}
    elif method == "embed_batch":
        texts = req.get("params", {{}}).get("texts", [])
        result = {{"dims": 384, "vectors": [[0.1] * 384 for _ in texts]}}
    elif method == "shutdown":
        result = {{"stopping": True}}
    else:
        result = {{}}

    response = {{
        "schema": "julie.embedding.sidecar",
        "version": 1,
        "request_id": req_id,
        "result": result,
    }}
    sys.stdout.write(json.dumps(response) + "\\n")
    sys.stdout.flush()

    if method == "shutdown":
        break
"#,
        ),
    )
    .expect("slow health sidecar script should be written");

    sidecar_script
}

#[cfg(feature = "embeddings-sidecar")]
struct EnvVarGuard {
    original: HashMap<String, Option<OsString>>,
}

#[cfg(feature = "embeddings-sidecar")]
impl EnvVarGuard {
    fn new() -> Self {
        Self {
            original: HashMap::new(),
        }
    }

    fn set(&mut self, key: &str, value: impl Into<OsString>) {
        if !self.original.contains_key(key) {
            self.original.insert(key.to_string(), std::env::var_os(key));
        }
        let value = value.into();
        unsafe {
            std::env::set_var(key, &value);
        }
    }

    fn remove(&mut self, key: &str) {
        if !self.original.contains_key(key) {
            self.original.insert(key.to_string(), std::env::var_os(key));
        }
        unsafe {
            std::env::remove_var(key);
        }
    }
}

#[cfg(feature = "embeddings-sidecar")]
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        for (key, value) in self.original.drain() {
            match value {
                Some(original_value) => unsafe {
                    std::env::set_var(&key, original_value);
                },
                None => unsafe {
                    std::env::remove_var(&key);
                },
            }
        }
    }
}

#[cfg(feature = "embeddings-sidecar")]
#[tokio::test]
#[serial(embedding_env)]
async fn test_spawn_workspace_embedding_discards_init_result_after_workspace_switch() {
    let temp_dir_a = TempDir::new().unwrap();
    let marker_path = temp_dir_a.path().join("health.marker");
    let sidecar_script = write_slow_health_sidecar_script(&temp_dir_a, &marker_path);
    let temp_dir_b = TempDir::new().unwrap();

    let mut env_guard = EnvVarGuard::new();
    env_guard.set("JULIE_SKIP_EMBEDDINGS", "1");
    env_guard.set("JULIE_SKIP_SEARCH_INDEX", "1");

    let workspace_a = JulieWorkspace::initialize(temp_dir_a.path().to_path_buf())
        .await
        .expect("workspace A initialization should succeed");
    let workspace_b = JulieWorkspace::initialize(temp_dir_b.path().to_path_buf())
        .await
        .expect("workspace B initialization should succeed");
    let expected_root_b = workspace_b.root.clone();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    {
        let mut ws_guard = handler.workspace.write().await;
        *ws_guard = Some(workspace_a);
    }

    assert!(
        !marker_path.exists(),
        "slow sidecar marker should not exist before deferred init starts"
    );
    env_guard.set("JULIE_EMBEDDING_PROVIDER", "sidecar");
    env_guard.set("JULIE_EMBEDDING_SIDECAR_PROGRAM", test_python_interpreter());
    env_guard.set(
        "JULIE_EMBEDDING_SIDECAR_SCRIPT",
        sidecar_script.to_string_lossy().to_string(),
    );
    env_guard.set("JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS", "5000");
    env_guard.remove("JULIE_SKIP_EMBEDDINGS");

    let handler_for_init = handler.clone();
    let init_task = tokio::spawn(async move {
        spawn_workspace_embedding(&handler_for_init, "missing-workspace-id".to_string()).await
    });

    let marker_deadline = Instant::now() + Duration::from_secs(2);
    while !marker_path.exists() {
        if Instant::now() >= marker_deadline {
            panic!(
                "slow sidecar marker was never written; deferred init did not reach health check"
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    env_guard.remove("JULIE_EMBEDDING_PROVIDER");
    env_guard.remove("JULIE_EMBEDDING_SIDECAR_PROGRAM");
    env_guard.remove("JULIE_EMBEDDING_SIDECAR_SCRIPT");
    env_guard.remove("JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS");

    {
        let mut ws_guard = handler.workspace.write().await;
        *ws_guard = Some(workspace_b);
    }

    let _ = init_task.await;

    let ws_guard = handler.workspace.read().await;
    let active = ws_guard
        .as_ref()
        .expect("active workspace should remain set");
    assert_eq!(
        active.root, expected_root_b,
        "active workspace should be workspace B after switch"
    );
    assert!(
        active.embedding_provider.is_none(),
        "stale init result from workspace A must not be published to workspace B"
    );
    assert!(
        active.embedding_runtime_status.is_none(),
        "stale runtime status from workspace A must not be published to workspace B"
    );
}

#[cfg(feature = "embeddings-sidecar")]
#[tokio::test]
#[serial(embedding_env)]
async fn test_spawn_workspace_embedding_does_not_hold_write_lock_during_provider_init() {
    let temp_dir = TempDir::new().unwrap();
    let marker_path = temp_dir.path().join("health.marker");
    let sidecar_script = write_slow_health_sidecar_script(&temp_dir, &marker_path);

    let mut env_guard = EnvVarGuard::new();
    env_guard.set("JULIE_SKIP_EMBEDDINGS", "1");
    env_guard.set("JULIE_SKIP_SEARCH_INDEX", "1");

    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .expect("workspace initialization should succeed");

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    {
        let mut ws_guard = handler.workspace.write().await;
        *ws_guard = Some(workspace);
    }

    assert!(
        !marker_path.exists(),
        "slow sidecar marker should not exist before deferred init starts"
    );
    env_guard.set("JULIE_EMBEDDING_PROVIDER", "sidecar");
    env_guard.set("JULIE_EMBEDDING_SIDECAR_PROGRAM", test_python_interpreter());
    env_guard.set(
        "JULIE_EMBEDDING_SIDECAR_SCRIPT",
        sidecar_script.to_string_lossy().to_string(),
    );
    env_guard.set("JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS", "5000");
    env_guard.remove("JULIE_SKIP_EMBEDDINGS");

    let handler_for_init = handler.clone();
    let init_task = tokio::spawn(async move {
        spawn_workspace_embedding(&handler_for_init, "missing-workspace-id".to_string()).await
    });

    let marker_deadline = Instant::now() + Duration::from_secs(2);
    while !marker_path.exists() {
        if Instant::now() >= marker_deadline {
            panic!(
                "slow sidecar marker was never written; deferred init did not reach health check"
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    env_guard.remove("JULIE_EMBEDDING_PROVIDER");
    env_guard.remove("JULIE_EMBEDDING_SIDECAR_PROGRAM");
    env_guard.remove("JULIE_EMBEDDING_SIDECAR_SCRIPT");
    env_guard.remove("JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS");

    let read_lock_result =
        tokio::time::timeout(Duration::from_millis(75), handler.workspace.read()).await;
    assert!(
        read_lock_result.is_ok(),
        "workspace read lock should remain available while provider init is running"
    );

    init_task.abort();
    let _ = init_task.await;
}

#[cfg(feature = "embeddings-sidecar")]
#[tokio::test]
#[serial(embedding_env)]
async fn test_spawn_workspace_embedding_skips_stdio_reinit_when_runtime_status_already_set() {
    let temp_dir = TempDir::new().unwrap();
    let marker_path = temp_dir.path().join("health.marker");
    let sidecar_script = write_slow_health_sidecar_script(&temp_dir, &marker_path);

    let mut env_guard = EnvVarGuard::new();
    env_guard.set("JULIE_SKIP_EMBEDDINGS", "1");
    env_guard.set("JULIE_SKIP_SEARCH_INDEX", "1");

    let mut workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .expect("workspace initialization should succeed");
    workspace.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
        requested_backend: EmbeddingBackend::Sidecar,
        resolved_backend: EmbeddingBackend::Sidecar,
        accelerated: false,
        degraded_reason: Some("prior init failed; stay in keyword-only mode".to_string()),
    });

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    {
        let mut ws_guard = handler.workspace.write().await;
        *ws_guard = Some(workspace);
    }

    assert!(
        !marker_path.exists(),
        "slow sidecar marker should not exist before spawn_workspace_embedding runs"
    );
    env_guard.set("JULIE_EMBEDDING_PROVIDER", "sidecar");
    env_guard.set("JULIE_EMBEDDING_SIDECAR_PROGRAM", test_python_interpreter());
    env_guard.set(
        "JULIE_EMBEDDING_SIDECAR_SCRIPT",
        sidecar_script.to_string_lossy().to_string(),
    );
    env_guard.set("JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS", "5000");
    env_guard.remove("JULIE_SKIP_EMBEDDINGS");

    let embedded_count =
        spawn_workspace_embedding(&handler, "missing-workspace-id".to_string()).await;

    assert_eq!(
        embedded_count, 0,
        "embedding should be skipped when runtime status already records an unavailable provider"
    );
    assert!(
        !marker_path.exists(),
        "spawn_workspace_embedding should not retry stdio provider init when runtime status is already set"
    );

    let ws_guard = handler.workspace.read().await;
    let active = ws_guard
        .as_ref()
        .expect("active workspace should remain set");
    assert!(
        active.embedding_provider.is_none(),
        "workspace should remain without a provider after the bounded degraded outcome"
    );
    assert!(
        active
            .embedding_runtime_status
            .as_ref()
            .and_then(|status| status.degraded_reason.as_deref())
            .is_some_and(|reason| reason.contains("keyword-only")),
        "runtime status should remain intact after spawn_workspace_embedding skips reinit"
    );
}

#[cfg(feature = "embeddings-sidecar")]
#[tokio::test]
#[serial(embedding_env)]
async fn test_spawn_workspace_embedding_skips_when_daemon_service_is_unavailable() {
    let temp_dir = TempDir::new().unwrap();
    let marker_path = temp_dir.path().join("health.marker");

    let mut env_guard = EnvVarGuard::new();
    env_guard.set("JULIE_SKIP_EMBEDDINGS", "1");
    env_guard.set("JULIE_SKIP_SEARCH_INDEX", "1");

    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .expect("workspace initialization should succeed");

    let mut handler = JulieServerHandler::new_for_test().await.unwrap();
    handler.embedding_service = Some(Arc::new(EmbeddingService::initializing()));
    handler
        .embedding_service
        .as_ref()
        .expect("embedding service should be set")
        .publish_unavailable(
            "test: shared daemon embedding unavailable".to_string(),
            Some(EmbeddingRuntimeStatus {
                requested_backend: EmbeddingBackend::Auto,
                resolved_backend: EmbeddingBackend::Sidecar,
                accelerated: false,
                degraded_reason: Some("shared daemon embedding unavailable".to_string()),
            }),
        );
    {
        let mut ws_guard = handler.workspace.write().await;
        *ws_guard = Some(workspace);
    }

    let embedded_count =
        spawn_workspace_embedding(&handler, "missing-workspace-id".to_string()).await;

    assert_eq!(
        embedded_count, 0,
        "embedding should be skipped when daemon service is unavailable"
    );
    assert!(
        !marker_path.exists(),
        "daemon-mode spawn_workspace_embedding should not fall through to stdio provider init after shared service failure"
    );
}

#[cfg(feature = "embeddings-sidecar")]
#[tokio::test(start_paused = true)]
#[serial(embedding_env)]
async fn test_spawn_workspace_embedding_skips_when_daemon_service_times_out() {
    let temp_dir = TempDir::new().unwrap();
    let marker_path = temp_dir.path().join("health.marker");

    let mut env_guard = EnvVarGuard::new();
    env_guard.set("JULIE_SKIP_EMBEDDINGS", "1");
    env_guard.set("JULIE_SKIP_SEARCH_INDEX", "1");

    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .expect("workspace initialization should succeed");

    let mut handler = JulieServerHandler::new_for_test().await.unwrap();
    handler.embedding_service = Some(Arc::new(EmbeddingService::initializing()));
    {
        let mut ws_guard = handler.workspace.write().await;
        *ws_guard = Some(workspace);
    }

    let task = tokio::spawn(async move {
        spawn_workspace_embedding(&handler, "missing-workspace-id".to_string()).await
    });

    tokio::time::advance(Duration::from_secs(121)).await;

    let embedded_count = task
        .await
        .expect("spawn_workspace_embedding task should not panic");
    assert_eq!(
        embedded_count, 0,
        "embedding should be skipped after daemon wait timeout"
    );
    assert!(
        !marker_path.exists(),
        "daemon-mode spawn_workspace_embedding should not fall through to stdio init after shared service timeout"
    );
}
