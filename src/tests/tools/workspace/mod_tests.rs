//! Tests for `workspace::JulieWorkspace` extracted from the implementation module.

use crate::embeddings::{DeviceInfo, EmbeddingBackend, EmbeddingProvider, EmbeddingRuntimeStatus};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt};
use crate::tools::workspace::ManageWorkspaceTool;
#[cfg(feature = "embeddings-sidecar")]
use crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding;
use crate::workspace::JulieWorkspace;
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
    env_guard.set("JULIE_EMBEDDING_PROVIDER", "sidecar");
    env_guard.set("JULIE_EMBEDDING_SIDECAR_PROGRAM", test_python_interpreter());
    env_guard.set(
        "JULIE_EMBEDDING_SIDECAR_SCRIPT",
        sidecar_script.to_string_lossy().to_string(),
    );
    env_guard.set("JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS", "5000");
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

    env_guard.remove("JULIE_SKIP_EMBEDDINGS");
    assert!(
        !marker_path.exists(),
        "slow sidecar marker should not exist before deferred init starts"
    );

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
    env_guard.set("JULIE_EMBEDDING_PROVIDER", "sidecar");
    env_guard.set("JULIE_EMBEDDING_SIDECAR_PROGRAM", test_python_interpreter());
    env_guard.set(
        "JULIE_EMBEDDING_SIDECAR_SCRIPT",
        sidecar_script.to_string_lossy().to_string(),
    );
    env_guard.set("JULIE_EMBEDDING_SIDECAR_INIT_TIMEOUT_MS", "5000");
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

    env_guard.remove("JULIE_SKIP_EMBEDDINGS");
    assert!(
        !marker_path.exists(),
        "slow sidecar marker should not exist before deferred init starts"
    );

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

    let read_lock_result =
        tokio::time::timeout(Duration::from_millis(75), handler.workspace.read()).await;
    assert!(
        read_lock_result.is_ok(),
        "workspace read lock should remain available while provider init is running"
    );

    init_task.abort();
    let _ = init_task.await;
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
            resolved_backend: EmbeddingBackend::Ort,
            accelerated: false,
            degraded_reason: Some("ORT fallback to CPU after DirectML init failure".to_string()),
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
        health.contains("backend: ort") || health.contains("Backend: ort"),
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
        health_lower.contains("degraded") && health.contains("DirectML"),
        "health output should include degraded reason: {health}"
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
            resolved_backend: EmbeddingBackend::Ort,
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
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
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

/// Regression test for Bug #1: handle_add_command must update workspace statistics
///
/// Bug: When adding a reference workspace, the statistics (file_count, symbol_count)
/// were never updated in the registry after indexing, so `manage_workspace list`
/// would always show 0 files and 0 symbols even though the database had data.
///
/// Root cause: handle_add_command called index_workspace_files() and received correct
/// counts, but never called registry_service.update_workspace_statistics().
///
/// Fix: Added update_workspace_statistics() call after successful indexing, mirroring
/// the implementation in handle_refresh_command.
#[tokio::test]
async fn test_add_workspace_updates_statistics() {
    use crate::workspace::registry_service::WorkspaceRegistryService;

    // Skip background tasks for this test
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    // Setup: Create test workspaces with actual files
    let primary_dir = TempDir::new().unwrap();
    let reference_dir = TempDir::new().unwrap();

    // Create a simple test file in reference workspace
    // NOTE: Avoid macro invocations (e.g. println!) in test fixtures — the Rust
    // extractor captures them as symbols, which inflates counts unexpectedly.
    let test_file = reference_dir.path().join("test.rs");
    fs::write(
        &test_file,
        r#"
fn hello_world() {
    let _ = 42;
}

fn goodbye_world() {
    let _ = 99;
}
        "#,
    )
    .unwrap();

    // Initialize primary workspace
    let _primary_workspace = JulieWorkspace::initialize(primary_dir.path().to_path_buf())
        .await
        .unwrap();

    // Create handler (simulates the server context)
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(
            Some(primary_dir.path().to_str().unwrap().to_string()),
            true,
        )
        .await
        .unwrap();

    // Add reference workspace using ManageWorkspaceTool
    let tool = ManageWorkspaceTool {
        operation: "add".to_string(),
        path: Some(reference_dir.path().to_str().unwrap().to_string()),
        name: Some("test-workspace".to_string()),
        force: None,
        workspace_id: None,
        detailed: None,
    };

    let result = tool
        .handle_add_command(
            &handler,
            reference_dir.path().to_str().unwrap(),
            Some("test-workspace".to_string()),
        )
        .await;

    assert!(result.is_ok(), "handle_add_command failed: {:?}", result);

    // Verify: Check registry statistics directly
    let primary_workspace = handler.get_workspace().await.unwrap().unwrap();
    let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());
    let workspaces = registry_service.get_all_workspaces().await.unwrap();

    // Find the reference workspace we just added
    let reference_ws = workspaces
        .iter()
        .find(|ws| {
            matches!(
                ws.workspace_type,
                crate::workspace::registry::WorkspaceType::Reference
            )
        })
        .expect("Reference workspace not found in registry");

    // BUG #1: These were 0 before the fix
    assert!(
        reference_ws.file_count > 0,
        "Bug #1 regression: file_count is {}, should be > 0 after indexing",
        reference_ws.file_count
    );

    assert!(
        reference_ws.symbol_count > 0,
        "Bug #1 regression: symbol_count is {}, should be > 0 after indexing (file has 2 functions)",
        reference_ws.symbol_count
    );

    // Additional validation: symbol count should match the 2 functions in our test file
    assert_eq!(
        reference_ws.symbol_count, 2,
        "Expected 2 symbols (hello_world and goodbye_world), got {}",
        reference_ws.symbol_count
    );

    assert_eq!(
        reference_ws.file_count, 1,
        "Expected 1 file (test.rs), got {}",
        reference_ws.file_count
    );
}

#[tokio::test]
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

#[tokio::test]
async fn test_primary_refresh_schedules_embedding_when_provider_available() {
    use crate::workspace::registry_service::WorkspaceRegistryService;

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("lib.rs");
    fs::write(&test_file, "fn gamma() {}\nfn delta() {}\n").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
    }

    // Ensure workspace is indexed and registered first.
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await.unwrap();

    let workspace = handler.get_workspace().await.unwrap().unwrap();
    let registry_service = WorkspaceRegistryService::new(workspace.root.clone());
    let primary_id = registry_service
        .get_primary_workspace_id()
        .await
        .unwrap()
        .expect("primary workspace id should exist");

    let refresh_tool = ManageWorkspaceTool {
        operation: "refresh".to_string(),
        path: None,
        force: Some(true),
        name: None,
        workspace_id: Some(primary_id),
        detailed: None,
    };

    let result = refresh_tool.call_tool(&handler).await.unwrap();
    let message = extract_text_from_result(&result);

    assert!(
        message.contains("Embedding") && message.contains("background"),
        "Primary refresh should schedule embeddings when provider is available. Message: {message}"
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

    // Try to index again with force=false - should skip
    let result = tool.call_tool(&handler).await.unwrap();
    let result_text = extract_text_from_result(&result);

    // Should see "already indexed" message with symbol count > 0
    assert!(
        result_text.contains("already indexed"),
        "Should skip re-indexing when database has symbols, got: {}",
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
