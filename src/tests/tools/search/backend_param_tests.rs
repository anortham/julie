use anyhow::Result;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

use crate::daemon::embedding_service::EmbeddingService;
use crate::embeddings::{DeviceInfo, EmbeddingProvider};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::search::{FastSearchTool, SearchBackend};
use crate::tools::workspace::ManageWorkspaceTool;

fn extract_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content| content.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn mark_search_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

async fn index_workspace(workspace_path: &std::path::Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    mark_search_ready(&handler).await;
    Ok(handler)
}

struct StaticProvider;

fn semantic_target_vector() -> Vec<f32> {
    let mut vector = vec![0.0_f32; 768];
    vector[0] = 1.0;
    vector
}

fn semantic_unrelated_vector() -> Vec<f32> {
    let mut vector = vec![0.0_f32; 768];
    vector[1] = 1.0;
    vector
}

impl EmbeddingProvider for StaticProvider {
    fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(semantic_target_vector())
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| semantic_target_vector()).collect())
    }

    fn dimensions(&self) -> usize {
        768
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "test".to_string(),
            device: "cpu".to_string(),
            model_name: "static-fast-search-backend".to_string(),
            dimensions: 768,
        }
    }
}

async fn semantic_workspace_with_embeddings() -> Result<(TempDir, JulieServerHandler)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    fs::create_dir_all(workspace_path.join("src"))?;
    fs::write(
        workspace_path.join("src/lib.rs"),
        "pub fn semantic_backend_target() {}\npub fn unrelated_backend_symbol() {}\n",
    )?;
    fs::write(
        workspace_path.join("src/notes.rs"),
        "// conceptual permissions handoff appears here only as lexical text\n",
    )?;

    let mut handler = index_workspace(workspace_path).await?;
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(StaticProvider);
    handler.embedding_service = Some(Arc::new(EmbeddingService::initialize_for_test(Some(
        provider,
    ))));

    let mut db = handler.primary_pooled_database().await?;
    let symbols = db.get_all_symbols()?;
    let target_id = symbols
        .iter()
        .find(|symbol| symbol.name == "semantic_backend_target")
        .map(|symbol| symbol.id.clone())
        .expect("indexed target symbol");
    let unrelated_id = symbols
        .iter()
        .find(|symbol| symbol.name == "unrelated_backend_symbol")
        .map(|symbol| symbol.id.clone())
        .expect("indexed unrelated symbol");
    db.store_embeddings(&[
        (target_id, semantic_target_vector()),
        (unrelated_id, semantic_unrelated_vector()),
    ])?;
    drop(db);

    Ok((temp_dir, handler))
}

#[test]
fn fast_search_deserializes_and_serializes_explicit_semantic_backend() {
    let tool: FastSearchTool =
        serde_json::from_str(r#"{"query":"find request auth flow","backend":"semantic"}"#)
            .expect("semantic backend should deserialize");

    let serialized = serde_json::to_value(&tool).expect("fast_search should serialize");

    assert_eq!(serialized["backend"], "semantic");
}

#[test]
fn fast_search_rejects_unknown_backend() {
    let error = serde_json::from_str::<FastSearchTool>(r#"{"query":"needle","backend":"vector"}"#)
        .expect_err("unknown backend values should be rejected");

    assert!(
        error.to_string().contains("unknown variant")
            || error.to_string().contains("unknown backend"),
        "unexpected error for invalid backend: {error}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn semantic_backend_falls_back_to_lexical_when_provider_is_unavailable() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    fs::create_dir_all(workspace_path.join("src"))?;
    fs::write(
        workspace_path.join("src/lib.rs"),
        "pub fn lexical_backend_marker() {}\n",
    )?;

    let mut handler = index_workspace(workspace_path).await?;
    handler.embedding_service = Some(Arc::new(EmbeddingService::initialize_for_test(None)));

    let run = FastSearchTool {
        query: "lexical_backend_marker".to_string(),
        backend: Some(SearchBackend::Semantic),
        return_format: "locations".to_string(),
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run.execution.expect("fast_search should return execution");
    let text = extract_text(&run.result);

    assert!(
        execution
            .hits
            .iter()
            .any(|hit| hit.name == "lexical_backend_marker"),
        "explicit semantic fallback should still return lexical hits, got: {:?}",
        execution
            .hits
            .iter()
            .map(|hit| hit.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        execution.trace.backend_fallback,
        "trace should record explicit backend fallback"
    );
    assert!(
        text.contains("backend=semantic") && text.contains("fell back to lexical"),
        "fallback response should tell the caller it used lexical search, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn semantic_backend_returns_symbol_hits_and_preserves_symbol_kind() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let execution = FastSearchTool {
        query: "conceptual permissions handoff".to_string(),
        backend: Some(SearchBackend::Semantic),
        limit: 1,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("semantic backend should return execution");

    assert!(!execution.trace.backend_fallback);
    assert_eq!(execution.trace.strategy_id, "fast_search_semantic");
    let top = execution.hits.first().expect("semantic backend should hit");
    assert_eq!(top.name, "semantic_backend_target");
    assert_eq!(
        top.kind, "function",
        "semantic backend must preserve actual symbol kind"
    );
    assert!(
        top.symbol_id.is_some(),
        "semantic backend result should remain symbol-backed"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn semantic_backend_locations_render_semantic_hits_not_lexical_line_mode() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let run = FastSearchTool {
        query: "conceptual permissions handoff".to_string(),
        backend: Some(SearchBackend::Semantic),
        return_format: "locations".to_string(),
        limit: 1,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .as_ref()
        .expect("semantic backend should return execution");
    let text = extract_text(&run.result);

    assert_eq!(execution.trace.strategy_id, "fast_search_semantic");
    assert_eq!(
        execution.hits.first().map(|hit| hit.name.as_str()),
        Some("semantic_backend_target")
    );
    assert!(
        text.contains("src/lib.rs:1"),
        "locations mode should render the semantic symbol hit, got:\n{text}"
    );
    assert!(
        !text.contains("src/notes.rs"),
        "locations mode must not replace semantic backend output with lexical line-mode hits, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn lexical_zero_hits_use_semantic_fallback_when_embeddings_are_ready() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let run = FastSearchTool {
        query: "ObscureProbe".to_string(),
        return_format: "locations".to_string(),
        limit: 1,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .as_ref()
        .expect("fast_search should return execution");
    let text = extract_text(&run.result);

    assert_eq!(execution.trace.strategy_id, "fast_search_semantic_fallback");
    assert_eq!(
        execution.hits.first().map(|hit| hit.name.as_str()),
        Some("semantic_backend_target")
    );
    assert!(
        text.contains("No lexical results. Showing semantic fallback candidates."),
        "fallback response should make the backend switch explicit, got:\n{text}"
    );
    assert!(
        text.contains("src/lib.rs:1"),
        "semantic fallback should render the semantic symbol hit, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn lexical_zero_hits_skip_semantic_fallback_for_path_queries() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let run = FastSearchTool {
        query: "missing/quasar".to_string(),
        return_format: "locations".to_string(),
        limit: 1,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .as_ref()
        .expect("fast_search should return execution");
    let text = extract_text(&run.result);

    assert_eq!(execution.trace.strategy_id, "search_unified");
    assert!(
        !text.contains("semantic_backend_target"),
        "path-shaped miss should keep the lexical no-results response, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_lexical_zero_hits_do_not_use_semantic_fallback() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let run = FastSearchTool {
        query: "ObscureProbe".to_string(),
        backend: Some(SearchBackend::Lexical),
        return_format: "locations".to_string(),
        limit: 1,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .as_ref()
        .expect("fast_search should return execution");
    let text = extract_text(&run.result);

    assert_eq!(execution.trace.strategy_id, "search_unified");
    assert!(
        execution.hits.is_empty(),
        "explicit lexical misses should remain pure lexical for bakeoffs, got: {:?}",
        execution
            .hits
            .iter()
            .map(|hit| hit.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        !text.contains("semantic_backend_target"),
        "explicit lexical miss should not include semantic fallback output, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn lexical_zero_hits_skip_semantic_fallback_for_plain_language_noise() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let run = FastSearchTool {
        query: "obscure zeppelin quasar".to_string(),
        return_format: "locations".to_string(),
        limit: 1,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .as_ref()
        .expect("fast_search should return execution");
    let text = extract_text(&run.result);

    assert_eq!(execution.trace.strategy_id, "search_unified");
    assert!(
        execution.hits.is_empty(),
        "plain language misses should not be replaced with nearest semantic symbols, got: {:?}",
        execution
            .hits
            .iter()
            .map(|hit| hit.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        !text.contains("semantic_backend_target"),
        "plain language miss should keep the lexical no-results response, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn lexical_zero_hits_skip_semantic_fallback_with_file_pattern() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let run = FastSearchTool {
        query: "ObscureProbe".to_string(),
        file_pattern: Some("docs/**".to_string()),
        return_format: "locations".to_string(),
        limit: 1,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .as_ref()
        .expect("fast_search should return execution");
    let text = extract_text(&run.result);

    assert_eq!(execution.trace.strategy_id, "search_unified");
    assert!(
        execution.hits.is_empty(),
        "scoped misses should not be replaced with semantic symbols, got: {:?}",
        execution
            .hits
            .iter()
            .map(|hit| hit.name.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        !text.contains("semantic_backend_target"),
        "scoped miss should keep the lexical no-results response, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn hybrid_backend_returns_symbol_hits_without_fallback() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let execution = FastSearchTool {
        query: "conceptual permissions handoff".to_string(),
        backend: Some(SearchBackend::Hybrid),
        limit: 1,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("hybrid backend should return execution");

    assert!(!execution.trace.backend_fallback);
    assert_eq!(execution.trace.strategy_id, "fast_search_hybrid");
    let top = execution.hits.first().expect("hybrid backend should hit");
    assert_eq!(top.name, "semantic_backend_target");
    assert_eq!(top.kind, "function");

    Ok(())
}
