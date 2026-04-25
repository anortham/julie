//! Tests for `workspace::JulieWorkspace` extracted from the implementation module.

#[cfg(feature = "embeddings-sidecar")]
use crate::daemon::embedding_service::EmbeddingService;
use crate::embeddings::{DeviceInfo, EmbeddingBackend, EmbeddingProvider, EmbeddingRuntimeStatus};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::workspace::ManageWorkspaceTool;
#[cfg(feature = "embeddings-sidecar")]
use crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding;
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
use std::sync::Arc;
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

#[tokio::test]
async fn test_workspace_initialization() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    // Check that .julie directory was created
    assert!(workspace.julie_dir.exists());

    // Check that all required root subdirectories exist
    // Note: Per-workspace directories (indexes/{workspace_id}/) are created on-demand during indexing
    assert!(
        workspace.julie_dir.join("indexes").exists(),
        "indexes/ root directory should exist"
    );
    // Note: models/ directory was removed in v2.0 (embeddings replaced by Tantivy)
    assert!(workspace.julie_dir.join("cache").exists());
    assert!(workspace.julie_dir.join("logs").exists());
    assert!(workspace.julie_dir.join("config").exists());

    // Check that config file was created
    assert!(workspace.julie_dir.join("config/julie.toml").exists());

    // Check that .gitignore was created to prevent accidental commits
    assert!(
        workspace.julie_dir.join(".gitignore").exists(),
        ".gitignore should be created in .julie directory"
    );
}

#[tokio::test]
async fn test_workspace_detection() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }
    let temp_dir = TempDir::new().unwrap();

    // Initialize workspace
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    drop(workspace);

    // Clean up any per-workspace directories if they exist
    let indexes_dir = temp_dir.path().join(".julie").join("indexes");
    if indexes_dir.exists() {
        let _ = fs::remove_dir_all(&indexes_dir);
        fs::create_dir_all(&indexes_dir).unwrap();
    }

    // Test detection from same directory
    let detected = JulieWorkspace::detect_and_load(temp_dir.path().to_path_buf())
        .await
        .unwrap();
    assert!(detected.is_some());

    // Test detection from subdirectory
    let subdir = temp_dir.path().join("subdir");
    fs::create_dir(&subdir).unwrap();
    let detected = JulieWorkspace::detect_and_load(subdir).await.unwrap();
    assert!(detected.is_some());
}

#[tokio::test]
async fn test_health_check() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }
    let temp_dir = TempDir::new().unwrap();
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    let health = workspace.health_check().unwrap();
    assert!(health.is_healthy());
    assert!(health.structure_valid);
    assert!(health.has_write_permissions);
}

#[tokio::test]
async fn test_health_snapshot_classifies_plane_states_for_local_workspace() {
    use crate::health::{
        DaemonLifecycleState, EmbeddingState, HealthChecker, HealthLevel, ProjectionFreshness,
        ProjectionState, WatcherState,
    };
    use crate::tools::workspace::indexing::state::{
        IndexingOperation, IndexingRepairReason, IndexingStage,
    };

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn plane_snapshot_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())
            .unwrap();
    let tantivy_dir = handler
        .workspace_tantivy_dir_for(&workspace_id)
        .await
        .unwrap();
    let meta_path = tantivy_dir.join("meta.json");
    if meta_path.exists() {
        fs::remove_file(meta_path).unwrap();
    }

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.search_index = None;
        ws.embedding_provider = None;
        ws.embedding_runtime_status = None;
        let mut indexing = ws.indexing_runtime.write().unwrap();
        indexing.begin_operation(IndexingOperation::Incremental);
        indexing.transition_stage(IndexingStage::Projecting);
        indexing.set_catchup_active(true);
        indexing.set_watcher_paused(true);
        indexing.set_dirty_projection_count(2);
        indexing.record_repair_reason(IndexingRepairReason::ProjectionFailure);
    }

    let snapshot = HealthChecker::system_snapshot(&handler).await.unwrap();

    assert_eq!(snapshot.overall, HealthLevel::Degraded);
    assert_eq!(
        snapshot.control_plane.daemon_state,
        DaemonLifecycleState::Direct
    );
    assert_eq!(snapshot.control_plane.watcher_state, WatcherState::Local);
    assert_eq!(
        snapshot.data_plane.canonical_store.level,
        HealthLevel::Ready
    );
    assert_eq!(
        snapshot.data_plane.search_projection.state,
        ProjectionState::Missing
    );
    assert_eq!(
        snapshot.data_plane.search_projection.freshness,
        ProjectionFreshness::RebuildRequired
    );
    assert_eq!(
        snapshot.data_plane.search_projection.level,
        HealthLevel::Degraded
    );
    assert_eq!(snapshot.data_plane.indexing.level, HealthLevel::Degraded);
    assert_eq!(
        snapshot.data_plane.indexing.active_operation.as_deref(),
        Some("catch_up")
    );
    assert_eq!(
        snapshot.data_plane.indexing.stage.as_deref(),
        Some("projecting")
    );
    assert!(snapshot.data_plane.indexing.catchup_active);
    assert!(snapshot.data_plane.indexing.watcher_paused);
    assert_eq!(snapshot.data_plane.indexing.dirty_projection_count, 2);
    assert!(snapshot.data_plane.indexing.repair_needed);
    assert!(
        snapshot
            .data_plane
            .indexing
            .repair_reasons
            .contains(&"projection_failure".to_string())
    );
    assert_eq!(
        snapshot.runtime_plane.embeddings.state,
        EmbeddingState::NotInitialized
    );
}

#[tokio::test]
async fn test_manage_workspace_health_reports_control_data_and_runtime_planes() {
    use crate::tools::workspace::indexing::state::{
        IndexingOperation, IndexingRepairReason, IndexingStage,
    };

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn plane_report_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())
            .unwrap();
    let tantivy_dir = handler
        .workspace_tantivy_dir_for(&workspace_id)
        .await
        .unwrap();
    let meta_path = tantivy_dir.join("meta.json");
    if meta_path.exists() {
        fs::remove_file(meta_path).unwrap();
    }

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.search_index = None;
        ws.embedding_provider = None;
        ws.embedding_runtime_status = None;
        let mut indexing = ws.indexing_runtime.write().unwrap();
        indexing.begin_operation(IndexingOperation::Incremental);
        indexing.transition_stage(IndexingStage::Projecting);
        indexing.set_catchup_active(true);
        indexing.set_watcher_paused(true);
        indexing.set_dirty_projection_count(2);
        indexing.record_repair_reason(IndexingRepairReason::ProjectionFailure);
    }

    let result = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    }
    .call_tool(&handler)
    .await
    .unwrap();
    let health = extract_text_from_result(&result);

    assert!(health.contains("Control Plane"), "{health}");
    assert!(health.contains("Data Plane"), "{health}");
    assert!(health.contains("Runtime Plane"), "{health}");
    assert!(health.contains("Daemon Status: DIRECT"), "{health}");
    assert!(health.contains("Watcher Status: LOCAL"), "{health}");
    assert!(health.contains("Projection Status: MISSING"), "{health}");
    assert!(
        health.contains("Projection Freshness: REBUILD REQUIRED"),
        "{health}"
    );
    assert!(
        health.contains("Projection Repair Needed: true"),
        "{health}"
    );
    assert!(health.contains("Indexing Operation: CATCH_UP"), "{health}");
    assert!(health.contains("Indexing Stage: PROJECTING"), "{health}");
    assert!(health.contains("Catch-Up Active: true"), "{health}");
    assert!(health.contains("Watcher Paused: true"), "{health}");
    assert!(health.contains("Dirty Projection Entries: 2"), "{health}");
    assert!(
        health.contains("Repair Reasons: projection_failure"),
        "{health}"
    );
    assert!(
        health.contains("Embedding Status: NOT INITIALIZED"),
        "{health}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_surfaces_embedding_runtime_status() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
        ws.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: false,
            degraded_reason: Some("CPU only: no GPU detected in sidecar runtime".to_string()),
        });
    }

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);
    let health_lower = health.to_ascii_lowercase();

    assert!(
        health.contains("Embedding Runtime"),
        "health output should include embedding runtime section: {health}"
    );
    assert!(
        health.contains("Embedding Status: DEGRADED"),
        "health output should mark degraded runtime as DEGRADED: {health}"
    );
    assert!(
        health.contains("runtime: pytorch-sidecar") || health.contains("Runtime: pytorch-sidecar"),
        "health output should include sidecar runtime identity: {health}"
    );
    assert!(
        health.contains("backend: sidecar") || health.contains("Backend: sidecar"),
        "health output should include resolved backend: {health}"
    );
    assert!(
        health.contains("device: cpu") || health.contains("Device: cpu"),
        "health output should include runtime device: {health}"
    );
    assert!(
        health.contains("accelerated: false") || health.contains("Accelerated: false"),
        "health output should include acceleration flag: {health}"
    );
    assert!(
        health_lower.contains("degraded") && health.contains("CPU only"),
        "health output should include degraded reason: {health}"
    );
    assert!(
        health.contains("Query Fallback: semantic"),
        "health output should describe the query fallback mode when embeddings are available: {health}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_reports_unavailable_when_provider_missing() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = None;
        ws.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: false,
            degraded_reason: Some("provider init failed".to_string()),
        });
    }

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);

    assert!(
        health.contains("Embedding Status: UNAVAILABLE"),
        "health output should report missing provider as UNAVAILABLE: {health}"
    );
    assert!(
        health.contains("runtime: unavailable") || health.contains("Runtime: unavailable"),
        "health output should include standardized runtime field when provider is missing: {health}"
    );
    assert!(
        health.contains("Degraded: provider init failed"),
        "health output should include standardized degraded field when provider is missing: {health}"
    );
    assert!(
        health.contains("Query Fallback: keyword-only"),
        "health output should describe keyword-only fallback when embeddings are unavailable: {health}"
    );
    assert!(
        health.contains("provider") || health.contains("Provider"),
        "health output should explain provider is missing: {health}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_reports_not_initialized_when_runtime_status_missing() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = None;
        ws.embedding_runtime_status = None;
    }

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);

    assert!(health.contains("Embedding Runtime"), "{health}");
    assert!(
        health.contains("Embedding Status: NOT INITIALIZED"),
        "{health}"
    );
    assert!(health.contains("Runtime: unavailable"), "{health}");
    assert!(health.contains("Backend: unresolved"), "{health}");
    assert!(health.contains("Device: unavailable"), "{health}");
    assert!(health.contains("Accelerated: false"), "{health}");
    assert!(health.contains("Degraded: none"), "{health}");
    assert!(health.contains("Query Fallback: keyword-only"), "{health}");
}

#[tokio::test]
async fn test_manage_workspace_health_reports_initialized_when_not_degraded() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
        ws.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: false,
            degraded_reason: None,
        });
    }

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);

    assert!(health.contains("Embedding Runtime"), "{health}");
    assert!(health.contains("Embedding Status: INITIALIZED"), "{health}");
    assert!(health.contains("Runtime: pytorch-sidecar"), "{health}");
    assert!(health.contains("Backend: sidecar"), "{health}");
    assert!(health.contains("Device: cpu"), "{health}");
    assert!(health.contains("Accelerated: false"), "{health}");
    assert!(health.contains("Degraded: none"), "{health}");
    assert!(health.contains("Query Fallback: semantic"), "{health}");
}

#[tokio::test]
async fn test_manage_workspace_health_uses_rebound_session_primary() {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::workspace::registry::generate_workspace_id;

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let loaded_primary_root = temp_dir.path().join("loaded-primary");
    let rebound_primary_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(&loaded_primary_root).unwrap();
    fs::create_dir_all(&rebound_primary_root).unwrap();
    fs::write(
        loaded_primary_root.join("main.rs"),
        "fn loaded_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let loaded_primary_path = loaded_primary_root.canonicalize().unwrap();
    let loaded_primary_path_str = loaded_primary_path.to_string_lossy().to_string();
    let loaded_primary_id = generate_workspace_id(&loaded_primary_path_str).unwrap();
    let loaded_primary_ws = pool
        .get_or_init(&loaded_primary_id, loaded_primary_path.clone())
        .await
        .unwrap();

    let handler = JulieServerHandler::new_with_shared_workspace(
        loaded_primary_ws,
        loaded_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(loaded_primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .unwrap();
    {
        let mut loaded_workspace = handler.workspace.write().await;
        loaded_workspace
            .as_mut()
            .expect("loaded workspace should exist")
            .search_index = None;
    }

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&loaded_primary_id, &loaded_primary_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();

    let rebound_ws = pool
        .get_or_init(&rebound_primary_id, rebound_primary_path.clone())
        .await
        .unwrap();
    {
        let mut rebound_guard = rebound_ws.db.as_ref().unwrap().lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "lib.rs".to_string(),
            language: "rust".to_string(),
            hash: "rebound_hash".to_string(),
            size: 32,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: None,
        };
        let symbol = crate::extractors::Symbol {
            id: "rebound_symbol".to_string(),
            name: "rebound_primary".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 20,
            start_byte: 0,
            end_byte: 20,
            signature: Some("fn rebound_primary()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        };
        rebound_guard
            .bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &rebound_primary_id)
            .unwrap();
    }

    handler.set_current_primary_binding(rebound_primary_id, rebound_primary_path);

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let health = extract_text_from_result(&result);

    assert!(
        health.contains("SQLite Status: HEALTHY"),
        "health should use rebound current primary database: {health}"
    );
    assert!(
        health.contains("1 symbols across 1 files"),
        "health should report rebound primary stats, not stale loaded workspace stats: {health}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_manage_workspace_health_keeps_primary_snapshot_after_completed_swap() {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::health::{HealthChecker, SystemStatus};
    use crate::workspace::registry::generate_workspace_id;
    use futures::poll;

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let original_root = temp_dir.path().join("loaded-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src")).unwrap();
    fs::create_dir_all(rebound_root.join("src")).unwrap();
    fs::write(
        original_root.join("src").join("main.rs"),
        "fn loaded_primary() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_root.join("src").join("lib.rs"),
        "fn rebound_primary() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let original_path = original_root.canonicalize().unwrap();
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str).unwrap();
    daemon_db
        .upsert_workspace(&original_id, &original_path_str, "ready")
        .unwrap();
    let original_ws = pool
        .get_or_init(&original_id, original_path.clone())
        .await
        .unwrap();
    let original_meta_path = indexes_dir
        .join(&original_id)
        .join("tantivy")
        .join("meta.json");
    if original_meta_path.exists() {
        fs::remove_file(&original_meta_path).unwrap();
    }
    let mut original_handler_ws = (*original_ws).clone();
    original_handler_ws.search_index = None;
    {
        let mut original_guard = original_ws.db.as_ref().unwrap().lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "src/main.rs".to_string(),
            language: "rust".to_string(),
            hash: "original_hash".to_string(),
            size: 24,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: None,
        };
        let symbol = crate::extractors::Symbol {
            id: "original_symbol".to_string(),
            name: "loaded_primary".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/main.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 19,
            start_byte: 0,
            end_byte: 19,
            signature: Some("fn loaded_primary()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        };
        original_guard
            .bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &original_id)
            .unwrap();
    }

    let handler = JulieServerHandler::new_with_shared_workspace(
        Arc::new(original_handler_ws),
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .unwrap();

    let rebound_path = rebound_root.canonicalize().unwrap();
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str).unwrap();
    daemon_db
        .upsert_workspace(&rebound_id, &rebound_path_str, "ready")
        .unwrap();
    pool.get_or_init(&rebound_id, rebound_path.clone())
        .await
        .unwrap();

    let workspace_write_guard = handler.workspace.write().await;
    let mut readiness_future = Box::pin(HealthChecker::check_system_readiness(&handler, None));
    assert!(
        poll!(readiness_future.as_mut()).is_pending(),
        "health check should block on the first await while the workspace lock is held"
    );

    handler.set_current_primary_binding(rebound_id, rebound_path);
    drop(workspace_write_guard);
    assert!(
        !handler.is_primary_workspace_swap_in_progress(),
        "swap should be completed before the readiness future resumes"
    );

    match readiness_future.await.unwrap() {
        SystemStatus::SqliteOnly { symbol_count } => {
            assert_eq!(
                symbol_count, 1,
                "health should stay bound to the original snapshot"
            )
        }
        other => panic!("expected SqliteOnly from the original primary snapshot, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_manage_workspace_health_detailed_uses_rebound_session_primary() {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::health::HealthChecker;
    use crate::workspace::registry::generate_workspace_id;

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let loaded_primary_root = temp_dir.path().join("loaded-primary-detailed");
    let rebound_primary_root = temp_dir.path().join("rebound-primary-detailed");
    fs::create_dir_all(&loaded_primary_root).unwrap();
    fs::create_dir_all(&rebound_primary_root).unwrap();
    fs::write(
        loaded_primary_root.join("main.rs"),
        "fn loaded_primary_detailed() {}\n",
    )
    .unwrap();
    fs::write(
        rebound_primary_root.join("lib.rs"),
        "fn rebound_primary_detailed() {}\n",
    )
    .unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir.clone(),
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let loaded_primary_path = loaded_primary_root.canonicalize().unwrap();
    let loaded_primary_path_str = loaded_primary_path.to_string_lossy().to_string();
    let loaded_primary_id = generate_workspace_id(&loaded_primary_path_str).unwrap();
    let loaded_primary_ws = pool
        .get_or_init(&loaded_primary_id, loaded_primary_path.clone())
        .await
        .unwrap();

    let handler = JulieServerHandler::new_with_shared_workspace(
        loaded_primary_ws,
        loaded_primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(loaded_primary_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .unwrap();

    let rebound_primary_path = rebound_primary_root.canonicalize().unwrap();
    let rebound_primary_path_str = rebound_primary_path.to_string_lossy().to_string();
    let rebound_primary_id = generate_workspace_id(&rebound_primary_path_str).unwrap();
    daemon_db
        .upsert_workspace(&loaded_primary_id, &loaded_primary_path_str, "ready")
        .unwrap();
    daemon_db
        .upsert_workspace(&rebound_primary_id, &rebound_primary_path_str, "ready")
        .unwrap();

    let rebound_ws = pool
        .get_or_init(&rebound_primary_id, rebound_primary_path.clone())
        .await
        .unwrap();
    {
        let mut rebound_guard = rebound_ws.db.as_ref().unwrap().lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "lib.rs".to_string(),
            language: "rust".to_string(),
            hash: "rebound_detailed_hash".to_string(),
            size: 41,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: None,
        };
        let symbol = crate::extractors::Symbol {
            id: "rebound_detailed_symbol".to_string(),
            name: "rebound_primary_detailed".to_string(),
            kind: crate::extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 29,
            start_byte: 0,
            end_byte: 29,
            signature: Some("fn rebound_primary_detailed()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        };
        rebound_guard
            .bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &rebound_primary_id)
            .unwrap();
    }

    handler.set_current_primary_binding(rebound_primary_id, rebound_primary_path);

    let report = HealthChecker::get_detailed_health_report(&handler)
        .await
        .unwrap();

    assert!(
        report.contains("📊 Database: 1 symbols, 1 files, 0 relationships"),
        "detailed health should use rebound current-primary stats, not the stale loaded workspace: {report}"
    );
    assert!(
        report.contains("Projection Workspace: rebound-primary-detailed_")
            && report.contains("Projection Freshness: REBUILD REQUIRED"),
        "detailed health should use rebound current-primary projection state instead of stale loaded workspace state: {report}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_loaded_primary_without_tantivy_is_sqlite_only() {
    use crate::health::{HealthChecker, SystemStatus};

    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn sqlite_only_loaded_primary() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())
            .unwrap();
    let tantivy_dir = handler
        .workspace_tantivy_dir_for(&workspace_id)
        .await
        .unwrap();
    let meta_path = tantivy_dir.join("meta.json");
    if meta_path.exists() {
        fs::remove_file(meta_path).unwrap();
    }

    let readiness = HealthChecker::check_system_readiness(&handler, None)
        .await
        .unwrap();
    match readiness {
        SystemStatus::SqliteOnly { symbol_count } => assert!(symbol_count > 0),
        other => panic!("expected SqliteOnly for loaded primary without Tantivy, got {other:?}"),
    }
}

#[tokio::test]
async fn test_manage_workspace_health_rejects_neutral_gap_without_primary_identity() {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn neutral_gap_health_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    handler.publish_loaded_workspace_swap_intent_for_test();

    let tool = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    };

    let err = tool
        .call_tool(&handler)
        .await
        .expect_err("neutral gap should reject primary health requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_cold_start_returns_index_first_guidance() {
    let handler = JulieServerHandler::new_for_test().await.unwrap();

    let result = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    }
    .call_tool(&handler)
    .await
    .unwrap();

    let health = extract_text_from_result(&result);

    assert!(
        health
            .contains("No workspace initialized. Run manage_workspace(operation=\"index\") first."),
        "cold start should keep index-first guidance, got: {health}"
    );
    assert!(
        !health.contains("Primary workspace identity unavailable during swap"),
        "cold start should not be classified as a swap gap: {health}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_true_swap_gap_uses_swap_gap_classification() {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn true_swap_gap_health_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    handler
        .publish_loaded_workspace_swap_teardown_gap_for_test()
        .await;

    let err = ManageWorkspaceTool {
        operation: "health".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: Some(false),
    }
    .call_tool(&handler)
    .await
    .expect_err("true swap gap should reject primary health requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn test_manage_workspace_health_triggers_roots_resolution_when_primary_missing() {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::extractors::SymbolKind;
    use crate::workspace::registry::generate_workspace_id;

    let temp_dir = TempDir::new().unwrap();
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir).unwrap();

    let startup_root = temp_dir.path().join("startup");
    let roots_root = temp_dir.path().join("roots");
    fs::create_dir_all(&startup_root).unwrap();
    fs::create_dir_all(&roots_root).unwrap();
    fs::write(startup_root.join("main.rs"), "fn startup() {}\n").unwrap();
    fs::write(roots_root.join("lib.rs"), "fn roots_health() {}\n").unwrap();

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db")).unwrap());
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let startup_path = startup_root.canonicalize().unwrap();
    let startup_id = generate_workspace_id(&startup_path.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&startup_id, &startup_path.to_string_lossy(), "ready")
        .unwrap();
    let startup_ws = pool
        .get_or_init(&startup_id, startup_path.clone())
        .await
        .unwrap();

    let roots_path = roots_root.canonicalize().unwrap();
    let roots_id = generate_workspace_id(&roots_path.to_string_lossy()).unwrap();
    daemon_db
        .upsert_workspace(&roots_id, &roots_path.to_string_lossy(), "ready")
        .unwrap();
    let roots_ws = pool
        .get_or_init(&roots_id, roots_path.clone())
        .await
        .unwrap();
    {
        let mut roots_db = roots_ws.db.as_ref().unwrap().lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "lib.rs".to_string(),
            language: "rust".to_string(),
            hash: "roots_health_hash".to_string(),
            size: 24,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 1,
            content: None,
        };
        let symbol = crate::extractors::Symbol {
            id: "roots_health_symbol".to_string(),
            name: "roots_health".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "lib.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 18,
            start_byte: 0,
            end_byte: 18,
            signature: Some("fn roots_health()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        };
        roots_db
            .bulk_store_fresh_atomic(&[file_info], &[symbol], &[], &[], &[], &roots_id)
            .unwrap();
    }

    let handler = JulieServerHandler::new_with_shared_workspace_startup_hint(
        startup_ws,
        crate::workspace::startup_hint::WorkspaceStartupHint {
            path: startup_path.clone(),
            source: Some(crate::workspace::startup_hint::WorkspaceStartupSource::Cwd),
        },
        Some(Arc::clone(&daemon_db)),
        Some(startup_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await
    .unwrap();
    handler.set_client_supports_workspace_roots_for_test(true);
    assert_eq!(handler.current_workspace_id(), None);

    let (server_transport, client_transport) = tokio::io::duplex(256);
    let service =
        serve_directly::<rmcp::RoleServer, _, _, _, _>(handler.clone(), server_transport, None);
    let (read_half, mut write_half) = tokio::io::split(client_transport);
    let mut lines = BufReader::new(read_half).lines();

    let roots_reply = async {
        match read_server_message(&mut lines).await {
            ServerJsonRpcMessage::Request(request) => match request.request {
                ServerRequest::ListRootsRequest(_) => {
                    send_json_line(
                        &mut write_half,
                        &serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": request.id,
                            "result": {
                                "roots": [{ "uri": format!("file://{}", roots_path.to_string_lossy()) }]
                            }
                        }),
                    )
                    .await;
                }
                other => panic!("unexpected server request: {other:?}"),
            },
            other => panic!("unexpected server message: {other:?}"),
        }
    };

    let health = <JulieServerHandler as ServerHandler>::call_tool(
        &handler,
        CallToolRequestParams::new("manage_workspace").with_arguments(
            serde_json::json!({
                "operation": "health",
                "detailed": false
            })
            .as_object()
            .expect("health args")
            .clone(),
        ),
        RequestContext::new(NumberOrString::Number(12), service.peer().clone()),
    );
    let (_, result) = tokio::join!(roots_reply, health);
    let result = result.unwrap();
    let text = extract_text_from_result(&result);

    assert!(
        text.contains("SQLite Status: HEALTHY"),
        "health should succeed after roots resolution: {text}"
    );
    assert!(
        text.contains("1 symbols across 1 files"),
        "health should report the roots-bound workspace stats: {text}"
    );
    assert_eq!(
        handler.current_workspace_id().as_deref(),
        Some(roots_id.as_str()),
        "health should bind the roots-selected current primary"
    );

    drop(write_half);
    drop(lines);
    let _ = service.cancel().await;
}

#[tokio::test]
async fn test_manage_workspace_index_rejects_neutral_gap_without_primary_identity() {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn neutral_gap_index_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    handler.publish_loaded_workspace_swap_intent_for_test();

    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let err = tool
        .call_tool(&handler)
        .await
        .expect_err("neutral gap should reject primary index requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test]
async fn test_manage_workspace_index_rejects_neutral_gap_without_primary_identity_after_teardown() {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path().to_path_buf();
    fs::create_dir_all(workspace_path.join("src")).unwrap();
    fs::write(
        workspace_path.join("src").join("main.rs"),
        "fn teardown_gap_index_target() {}\n",
    )
    .unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();

    handler
        .publish_loaded_workspace_swap_teardown_gap_for_test()
        .await;

    let err = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect_err("post-teardown swap gap should reject primary index requests");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore] // HANGS: Concurrent indexing stress test - not critical for CLI tools
// Run manually with: cargo test test_concurrent_manage_workspace --ignored
async fn test_concurrent_manage_workspace_index_does_not_lock_search_index() {
    // Skip search index initialization but allow Tantivy to initialize
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::remove_var("JULIE_SKIP_SEARCH_INDEX");
    }

    let workspace_path = std::env::current_dir()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let run_index = |path: String| async move {
        let handler = JulieServerHandler::new_for_test().await.unwrap();
        let tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(path),
            force: Some(true),
            name: None,
            workspace_id: None,
            detailed: None,
        };

        tool.call_tool(&handler)
            .await
            .map_err(|err| err.to_string())
    };

    let handle_a = tokio::spawn(run_index(workspace_path.clone()));
    let handle_b = tokio::spawn(run_index(workspace_path.clone()));

    let result_a = handle_a.await.unwrap();
    let result_b = handle_b.await.unwrap();

    assert!(
        result_a.is_ok(),
        "first index run failed with: {:?}",
        result_a
    );
    assert!(
        result_b.is_ok(),
        "second index run failed with: {:?}",
        result_b
    );
}

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_primary_index_schedules_embedding_when_provider_available() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    // Inject deterministic provider so embedding scheduling is enabled in test.
    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let message = extract_text_from_result(&result);

    assert!(
        message.contains("Embedding") && message.contains("background"),
        "Primary index should schedule embeddings when provider is available. Message: {message}"
    );
}

/// Regression test for Bug: "Workspace already indexed: 0 symbols"
///
/// Bug: The is_indexed flag could be true while the database had 0 symbols,
/// causing the nonsensical message "Workspace already indexed: 0 symbols".
///
/// Root cause: The is_indexed flag was checked before querying the database,
/// and if true, would return early even when symbol_count was 0.
///
/// Fix: Added validation to check if symbol_count == 0, and if so, clear the
/// is_indexed flag and proceed with indexing instead of returning early.
#[tokio::test]
async fn test_is_indexed_flag_with_empty_database() {
    // Skip background tasks
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = temp_dir.path().join("test.rs");
    fs::write(
        &test_file,
        r#"
fn test_function() {
    println!("test");
}
        "#,
    )
    .unwrap();

    // Initialize workspace and handler
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_str().unwrap().to_string()), true)
        .await
        .unwrap();

    // First index to populate the database
    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("Workspace indexing complete"),
        "First indexing should succeed, got: {}",
        result_text
    );

    // Verify is_indexed is true
    assert!(
        *handler.is_indexed.read().await,
        "is_indexed should be true after indexing"
    );

    // SIMULATE THE BUG: Manually clear the database while keeping is_indexed=true
    // This simulates scenarios like database corruption, manual deletion, or partial cleanup
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            // Clear all symbols to simulate empty database
            // Clear all symbols to simulate empty database
            db_lock.conn.execute("DELETE FROM symbols", []).unwrap();
        }
    }

    // Verify database is now empty
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert_eq!(count, 0, "Database should be empty after manual deletion");
        }
    }

    // Verify is_indexed flag is still true (simulating the bug condition)
    assert!(
        *handler.is_indexed.read().await,
        "is_indexed should still be true (bug condition)"
    );

    // NOW TEST THE FIX: Try to index again with force=false
    // Before the fix: Would return "Workspace already indexed: 0 symbols"
    // After the fix: Should detect empty database, clear flag, and proceed with indexing
    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    // THE FIX: Should NOT see "already indexed: 0 symbols"
    assert!(
        !result_text.contains("already indexed: 0 symbols"),
        "Bug regression: Should not see 'already indexed: 0 symbols', got: {}",
        result_text
    );

    // THE FIX: Should proceed with indexing and report success
    assert!(
        result_text.contains("Workspace indexing complete") || result_text.contains("symbols"),
        "Should re-index when database is empty, got: {}",
        result_text
    );
}

/// Test that when is_indexed=true AND database has symbols, indexing is correctly skipped
#[tokio::test]
async fn test_is_indexed_flag_with_populated_database() {
    // Skip background tasks
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = temp_dir.path().join("test.rs");
    fs::write(
        &test_file,
        r#"
fn test_function() {
    println!("test");
}
        "#,
    )
    .unwrap();

    // Initialize workspace and handler
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_str().unwrap().to_string()), true)
        .await
        .unwrap();

    // First index to populate the database
    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("Workspace indexing complete"),
        "First indexing should succeed"
    );

    // Verify is_indexed is true
    assert!(*handler.is_indexed.read().await);

    // Verify database has symbols
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert!(count > 0, "Database should have symbols");
        }
    }

    // Try to index again with force=false - should run incremental update
    // (catch-up indexing compares blake3 hashes; unchanged files are skipped)
    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    // Incremental re-index succeeds and still reports symbols
    assert!(
        result_text.contains("Workspace indexing complete"),
        "Incremental re-index should succeed, got: {}",
        result_text
    );

    assert!(
        !result_text.contains("0 symbols"),
        "Should NOT report 0 symbols, got: {}",
        result_text
    );
}

/// Test that force=true clears the is_indexed flag and performs re-indexing
#[tokio::test]
async fn test_force_reindex_clears_flag() {
    // Skip background tasks
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = temp_dir.path().join("test.rs");
    fs::write(
        &test_file,
        r#"
fn test_function() {
    println!("test");
}
        "#,
    )
    .unwrap();

    // Initialize workspace and handler
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_str().unwrap().to_string()), true)
        .await
        .unwrap();

    // First index
    let tool_no_force = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool_no_force.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(result_text.contains("Workspace indexing complete"));

    // Verify is_indexed is true
    assert!(*handler.is_indexed.read().await);

    // Force reindex
    let tool_force = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool_force.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    // Should complete indexing again (not skip)
    assert!(
        result_text.contains("Workspace indexing complete"),
        "Force reindex should complete indexing, got: {}",
        result_text
    );

    // Verify is_indexed is true after force reindex
    assert!(*handler.is_indexed.read().await);
}

/// Regression test for Bug: Incremental indexing skips files when database has 0 symbols
///
/// Bug: When database files table has file hashes but symbols table is empty,
/// incremental indexing considers files "unchanged" and skips them, resulting
/// in persistent 0 symbols even after re-indexing.
///
/// Root cause: filter_changed_files() only checks file hashes, not symbol count.
/// It doesn't detect the empty database condition and force full re-extraction.
///
/// Fix: Add check at start of filter_changed_files() to detect 0 symbols and
/// bypass incremental logic, returning all files for re-indexing.
#[tokio::test]
async fn test_incremental_indexing_detects_empty_database() {
    // Skip background tasks
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();

    // Create test files with actual code
    // NOTE: Avoid macro invocations (e.g. println!) — the Rust extractor captures
    // them as symbols, inflating counts beyond the intended function-only assertions.
    let test_file_1 = temp_dir.path().join("file1.rs");
    fs::write(
        &test_file_1,
        r#"
fn function_one() {
    let _ = 1;
}
        "#,
    )
    .unwrap();

    let test_file_2 = temp_dir.path().join("file2.rs");
    fs::write(
        &test_file_2,
        r#"
fn function_two() {
    let _ = 2;
}
        "#,
    )
    .unwrap();

    // Initialize workspace and handler
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_str().unwrap().to_string()), true)
        .await
        .unwrap();

    // First index to populate database
    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_str().unwrap().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("Workspace indexing complete"),
        "First indexing should succeed"
    );

    // Verify we have symbols
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert_eq!(count, 2, "Should have 2 symbols from 2 functions");
        }
    }

    // SIMULATE THE BUG: Clear symbols table while keeping files table intact
    // This simulates the condition where file hashes exist but no symbols are extracted
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            // Clear symbols but keep files table (file hashes remain)
            db_lock.conn.execute("DELETE FROM symbols", []).unwrap();

            // Verify files table still has entries
            let file_count: i64 = db_lock
                .conn
                .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
                .unwrap();
            assert!(file_count > 0, "Files table should still have entries");
        }
    }

    // Verify database is now empty (0 symbols) but files table has hashes
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert_eq!(
                count, 0,
                "Database should have 0 symbols after manual deletion"
            );
        }
    }

    // Clear is_indexed flag to force the indexing logic to run
    *handler.is_indexed.write().await = false;

    // NOW TEST THE FIX: Try to index again with force=false
    // Before the fix: Incremental logic sees matching file hashes, skips files → 0 symbols persist
    // After the fix: Should detect empty database, bypass incremental logic, re-extract all symbols
    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("Workspace indexing complete"),
        "Re-indexing should complete"
    );

    // THE FIX: Should have re-extracted symbols despite matching file hashes
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let db_lock = db.lock().unwrap();
            let count = db_lock.count_symbols_for_workspace().unwrap();
            assert_eq!(
                count, 2,
                "Bug regression: Incremental indexing should detect empty database and re-extract symbols, got {} symbols",
                count
            );
        }
    }
}

/// Regression test: refresh with no file changes should NOT trigger the full
/// embedding pipeline. Previously, every refresh unconditionally called
/// spawn_workspace_embedding, re-embedding ~2000 enriched symbols even when
/// nothing changed.
#[tokio::test]
async fn test_refresh_no_changes_skips_embedding_pipeline() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn hello() {}\nfn world() {}\n").unwrap();

    // Set up daemon database (refresh requires daemon mode)
    let daemon_db_dir = temp_dir.path().join(".julie");
    fs::create_dir_all(&daemon_db_dir).unwrap();
    let daemon_db = Arc::new(
        crate::daemon::database::DaemonDatabase::open(&daemon_db_dir.join("daemon.db")).unwrap(),
    );

    let workspace_path_str = temp_dir.path().to_string_lossy().to_string();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_path_str).unwrap();

    // Create handler with daemon_db
    let mut handler = JulieServerHandler::new_for_test().await.unwrap();
    handler.daemon_db = Some(daemon_db.clone());
    *handler
        .workspace_id
        .write()
        .unwrap_or_else(|p| p.into_inner()) = Some(workspace_id.clone());

    handler
        .initialize_workspace_with_force(Some(workspace_path_str.clone()), true)
        .await
        .unwrap();

    // Inject a real embedding provider so spawn_workspace_embedding would
    // return non-zero if called. Without this, the test could pass trivially
    // because no provider means embed_count=0 regardless of the gate.
    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    // Register workspace in daemon db
    daemon_db
        .upsert_workspace(&workspace_id, &workspace_path_str, "ready")
        .unwrap();

    // First: index the workspace so files are known
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path_str.clone()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let result = index_tool.call_tool(&handler).await.unwrap();
    let msg = extract_text_from_result(&result);
    assert!(
        msg.contains("Workspace indexing complete"),
        "Index should succeed: {msg}"
    );

    // Wait for background embedding to finish so the workspace has embeddings
    // before refreshing. Without this, the catch-up logic would (correctly)
    // schedule embedding because the workspace has symbols but 0 vectors.
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

    // Now: refresh with no changes and no force
    let refresh_tool = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: Some(workspace_id.clone()),
        detailed: None,
    };
    let result = refresh_tool.call_tool(&handler).await.unwrap();
    let msg = extract_text_from_result(&result);

    assert!(
        msg.contains("Already up-to-date"),
        "Refresh with no changes should report up-to-date: {msg}"
    );
    // The bug: embedding pipeline was triggered even when nothing changed
    assert!(
        !msg.contains("Embedding"),
        "Refresh with no changes should NOT trigger embedding pipeline: {msg}"
    );
}

#[tokio::test]
#[serial]
async fn test_incremental_index_triggers_catch_up_embedding_when_none_exist() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn alpha() {}\nfn beta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    // Inject provider so embedding can run
    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    // First index with force: creates symbols and spawns background embedding
    let tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let _ = tool.call_tool(&handler).await.unwrap();

    // Wait for background embedding task to finish
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

    // Clear all embeddings to simulate "sidecar wasn't ready during initial indexing"
    if let Ok(Some(workspace)) = handler.get_workspace().await {
        if let Some(db) = workspace.db.as_ref() {
            let mut db_lock = db.lock().unwrap();
            db_lock.clear_all_embeddings().unwrap();
            assert_eq!(
                db_lock.embedding_count().unwrap(),
                0,
                "embeddings should be cleared"
            );
            assert!(
                db_lock.count_symbols_for_workspace().unwrap() > 0,
                "symbols should still exist"
            );
        }
    }

    // Second index: force=false, incremental — no file changes detected
    let incremental_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    let result = incremental_tool.call_tool(&handler).await.unwrap();
    let message = extract_text_from_result(&result);

    assert!(
        message.contains("Embedding") && message.contains("background"),
        "Incremental index should schedule catch-up embedding when workspace has symbols but 0 embeddings. Message: {message}"
    );
}
