use crate::embeddings::{DeviceInfo, EmbeddingBackend, EmbeddingProvider, EmbeddingRuntimeStatus};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::registry::WorkspaceType;
use crate::workspace::registry_service::WorkspaceRegistryService;
use std::sync::Arc;
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

async fn register_primary_workspace(handler: &JulieServerHandler, path: &str) -> String {
    let primary_workspace = handler.get_workspace().await.unwrap().unwrap();
    let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());
    registry_service
        .register_workspace(path.to_string(), WorkspaceType::Primary)
        .await
        .unwrap()
        .id
}

async fn register_reference_workspace(handler: &JulieServerHandler, path: &str) -> String {
    let primary_workspace = handler.get_workspace().await.unwrap().unwrap();
    let registry_service = WorkspaceRegistryService::new(primary_workspace.root.clone());
    registry_service
        .register_workspace(path.to_string(), WorkspaceType::Reference)
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn test_manage_workspace_stats_surfaces_embedding_runtime_status() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new().await.unwrap();
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

    let workspace_id = register_primary_workspace(&handler, &temp_dir.path().to_string_lossy()).await;
    let tool = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: Some(workspace_id),
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let stats = extract_text_from_result(&result);

    assert!(stats.contains("Embedding Runtime"), "{stats}");
    assert!(stats.contains("Runtime: pytorch-sidecar"), "{stats}");
    assert!(stats.contains("Backend: ort"), "{stats}");
    assert!(stats.contains("Device: cpu"), "{stats}");
    assert!(stats.contains("Accelerated: false"), "{stats}");
    assert!(
        stats.contains("Degraded: ORT fallback to CPU after DirectML init failure"),
        "{stats}"
    );
}

#[tokio::test]
async fn test_manage_workspace_stats_reports_unavailable_when_provider_missing() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new().await.unwrap();
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

    let workspace_id = register_primary_workspace(&handler, &temp_dir.path().to_string_lossy()).await;
    let tool = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: Some(workspace_id),
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let stats = extract_text_from_result(&result);

    assert!(stats.contains("Embedding Runtime"), "{stats}");
    assert!(stats.contains("Runtime: unavailable"), "{stats}");
    assert!(stats.contains("Backend: ort"), "{stats}");
    assert!(stats.contains("Device: unavailable"), "{stats}");
    assert!(stats.contains("Accelerated: false"), "{stats}");
    assert!(stats.contains("Degraded: provider init failed"), "{stats}");
}

#[tokio::test]
async fn test_manage_workspace_stats_for_reference_workspace_uses_runtime_unavailable_fallback() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let primary_dir = TempDir::new().unwrap();
    let reference_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(primary_dir.path().to_string_lossy().to_string()), true)
        .await
        .unwrap();

    {
        let mut ws_guard = handler.workspace.write().await;
        let ws = ws_guard.as_mut().expect("workspace should be initialized");
        ws.embedding_provider = Some(Arc::new(NoopEmbeddingProvider));
        ws.embedding_runtime_status = Some(EmbeddingRuntimeStatus {
            requested_backend: EmbeddingBackend::Auto,
            resolved_backend: EmbeddingBackend::Sidecar,
            accelerated: true,
            degraded_reason: None,
        });
    }

    let workspace_id =
        register_reference_workspace(&handler, &reference_dir.path().to_string_lossy()).await;
    let tool = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: Some(workspace_id),
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let stats = extract_text_from_result(&result);

    assert!(stats.contains("Embedding Runtime"), "{stats}");
    assert!(stats.contains("Runtime: unavailable"), "{stats}");
    assert!(stats.contains("Backend: unavailable"), "{stats}");
    assert!(stats.contains("Device: unavailable"), "{stats}");
    assert!(stats.contains("Accelerated: unknown"), "{stats}");
    assert!(
        stats.contains(
            "Degraded: unknown (runtime metadata is only tracked for loaded primary workspace)"
        ),
        "{stats}"
    );
}

#[tokio::test]
async fn test_manage_workspace_stats_when_runtime_status_missing_reports_unresolved_fallback() {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let handler = JulieServerHandler::new().await.unwrap();
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

    let workspace_id = register_primary_workspace(&handler, &temp_dir.path().to_string_lossy()).await;
    let tool = ManageWorkspaceTool {
        operation: "stats".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: Some(workspace_id),
        detailed: None,
    };

    let result = tool.call_tool(&handler).await.unwrap();
    let stats = extract_text_from_result(&result);

    assert!(stats.contains("Embedding Runtime"), "{stats}");
    assert!(stats.contains("Runtime: unavailable"), "{stats}");
    assert!(stats.contains("Backend: unresolved"), "{stats}");
    assert!(stats.contains("Device: unavailable"), "{stats}");
    assert!(stats.contains("Accelerated: false"), "{stats}");
    assert!(
        stats.contains("Degraded: none (runtime metadata not initialized)"),
        "{stats}"
    );
}
