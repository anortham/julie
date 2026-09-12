use anyhow::Result;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

use crate::embeddings::{DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity};
use crate::mcp_compat::CallToolResult;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::search::{FastSearchTool, SearchBackend};
use julie_test_support::FakeToolContext;

fn extract_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content| content.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join("\n")
}

async fn index_workspace(workspace_path: &std::path::Path) -> Result<FakeToolContext> {
    snapshot_context(workspace_path)
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
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        Ok(semantic_target_vector())
    }

    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| semantic_target_vector()).collect())
    }

    fn dimensions(&self) -> usize {
        768
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock("static-fast-search-backend", 768))
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

async fn semantic_workspace_with_embeddings() -> Result<(TempDir, FakeToolContext)> {
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

    let provider: Arc<dyn EmbeddingProvider> = Arc::new(StaticProvider);
    let handler = index_workspace(workspace_path)
        .await?
        .with_embedding_provider(provider);
    handler
        .snapshot_fixture
        .as_ref()
        .expect("snapshot fixture")
        .store_named_vectors(
            &EncoderIdentity::mock("static-fast-search-backend", 768),
            &[
                ("semantic_backend_target", semantic_target_vector()),
                ("unrelated_backend_symbol", semantic_unrelated_vector()),
            ],
        )?;

    Ok((temp_dir, handler))
}

async fn semantic_workspace_without_vectors() -> Result<(TempDir, FakeToolContext)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    fs::create_dir_all(workspace_path.join("src"))?;
    fs::write(
        workspace_path.join("src/lib.rs"),
        "pub fn semantic_backend_target() {}\n",
    )?;
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(StaticProvider);
    let handler = index_workspace(workspace_path)
        .await?
        .with_embedding_provider(provider);
    Ok((temp_dir, handler))
}

async fn scoped_content_workspace() -> Result<(TempDir, FakeToolContext)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    fs::create_dir_all(workspace_path.join("materials"))?;
    fs::create_dir_all(workspace_path.join("outside"))?;
    fs::write(
        workspace_path.join("materials/routing.md"),
        "Workspace routing keeps each checkout in its own physical index.\n",
    )?;
    fs::write(
        workspace_path.join("materials/routing-copy.md"),
        "Workspace routing keeps each checkout in its own physical index. Secondary copy.\n",
    )?;
    fs::write(
        workspace_path.join("materials/routing.json"),
        r#"{"note":"workspace routing keeps configuration isolated"}"#,
    )?;
    fs::write(
        workspace_path.join("materials/routing.rs"),
        "// Workspace routing source comment describes checkout isolation.\npub fn workspace_routing_candidate() {}\n",
    )?;
    fs::write(
        workspace_path.join("materials/routing_test.rs"),
        "// Workspace routing source comment belongs to a test.\npub fn workspace_routing_test_candidate() {}\n",
    )?;
    fs::write(
        workspace_path.join("outside/routing.md"),
        "Workspace routing outside the requested scope.\n",
    )?;

    let provider: Arc<dyn EmbeddingProvider> = Arc::new(StaticProvider);
    let handler = index_workspace(workspace_path)
        .await?
        .with_embedding_provider(provider);
    handler
        .snapshot_fixture
        .as_ref()
        .expect("snapshot fixture")
        .store_named_vectors(
            &EncoderIdentity::mock("static-fast-search-backend", 768),
            &[("workspace_routing_candidate", semantic_target_vector())],
        )?;

    Ok((temp_dir, handler))
}

#[tokio::test(flavor = "multi_thread")]
async fn required_semantics_report_not_ready_without_an_encoder_row() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_without_vectors().await?;

    let run = FastSearchTool {
        query: "conceptual permissions handoff".to_string(),
        backend: Some(SearchBackend::Semantic),
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Required),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await;
    let error = match run {
        Ok(_) => panic!("required semantics must fail closed without vectors"),
        Err(error) => error,
    };

    assert!(
        error.to_string().contains("SEMANTICS_NOT_READY"),
        "expected SEMANTICS_NOT_READY, got: {error}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn required_semantics_are_ready_with_an_encoder_row_and_vectors() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let execution = FastSearchTool {
        query: "conceptual permissions handoff".to_string(),
        backend: Some(SearchBackend::Semantic),
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Required),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("semantic backend should return execution");

    assert_eq!(execution.trace.strategy_id, "fast_search_semantic");
    assert_eq!(
        execution.hits.first().map(|hit| hit.name.as_str()),
        Some("semantic_backend_target")
    );
    Ok(())
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

    let handler = index_workspace(workspace_path).await?;

    let run = FastSearchTool {
        query: "lexical_backend_marker".to_string(),
        backend: Some(SearchBackend::Semantic),
        return_format: "compact".to_string(),
        offset: 0,
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
        offset: 0,
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
        return_format: "compact".to_string(),
        offset: 0,
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
        return_format: "compact".to_string(),
        offset: 0,
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
        return_format: "compact".to_string(),
        offset: 0,
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
        return_format: "compact".to_string(),
        offset: 0,
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
async fn lexical_zero_hits_skip_semantic_fallback_for_a_single_plain_word() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let run = FastSearchTool {
        query: "zeppelin".to_string(),
        return_format: "compact".to_string(),
        offset: 0,
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
        return_format: "compact".to_string(),
        offset: 0,
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
        offset: 0,
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

#[test]
fn auto_prefers_semantic_for_nl_queries() {
    assert!(SearchBackend::auto_prefers_semantic(
        "where does the app create the router"
    ));
    for query in [
        "SearchBackend",
        "parse_query score_candidate",
        "src/search/backend.rs",
        "find backend.rs",
        "",
    ] {
        assert!(
            !SearchBackend::auto_prefers_semantic(query),
            "auto must stay lexical for {query:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_nl_query_runs_semantic_when_vectors_are_ready() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let run = FastSearchTool {
        query: "semantic backend target function".to_string(),
        return_format: "compact".to_string(),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .as_ref()
        .expect("auto backend should return execution");
    let text = extract_text(&run.result);

    assert_eq!(execution.trace.strategy_id, "fast_search_semantic");
    assert!(!execution.trace.backend_fallback);
    assert!(
        text.contains("(semantic)"),
        "auto-semantic output should be labeled semantic, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_nl_query_stays_lexical_without_vectors() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_without_vectors().await?;

    let run = FastSearchTool {
        query: "semantic backend target function".to_string(),
        return_format: "compact".to_string(),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .as_ref()
        .expect("auto backend should return execution");
    let text = extract_text(&run.result);

    assert_eq!(execution.trace.strategy_id, "search_unified");
    assert!(!execution.trace.backend_fallback);
    assert!(
        !text.contains("NOTE: backend="),
        "a silent auto fallback must not add a note, got:\n{text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_nl_query_falls_through_to_lexical_on_zero_semantic_hits() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let execution = FastSearchTool {
        query: "unrelated prose that matches nothing embedded".to_string(),
        language: Some("python".to_string()),
        return_format: "compact".to_string(),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("auto backend should return execution");

    assert_eq!(execution.trace.strategy_id, "search_unified");
    assert!(!execution.trace.backend_fallback);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_identifier_query_stays_lexical_with_vectors() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let execution = FastSearchTool {
        query: "semantic_backend_target".to_string(),
        return_format: "compact".to_string(),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("auto backend should return execution");

    assert_ne!(execution.trace.strategy_id, "fast_search_semantic");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_lexical_never_runs_hybrid() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let execution = FastSearchTool {
        query: "semantic backend target function".to_string(),
        backend: Some(SearchBackend::Lexical),
        return_format: "compact".to_string(),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?
    .execution
    .expect("explicit lexical should return execution");

    assert_eq!(execution.trace.strategy_id, "search_unified");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn required_semantics_on_auto_nl_query_without_vectors_reports_not_ready() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_without_vectors().await?;

    let run = FastSearchTool {
        query: "semantic backend target function".to_string(),
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Required),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await;

    let error = match run {
        Ok(_) => panic!("required semantics must fail closed without vectors"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("SEMANTICS_NOT_READY"),
        "expected SEMANTICS_NOT_READY, got: {error}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_nl_query_never_waits_for_embedding_provider_init() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let auto = FastSearchTool {
        query: "semantic backend target function".to_string(),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let auto_execution = auto
        .execution
        .as_ref()
        .expect("auto backend should return execution");
    assert_eq!(auto_execution.trace.strategy_id, "fast_search_semantic");
    assert_eq!(handler.ensure_embedding_provider_call_count(), 0);

    let explicit = FastSearchTool {
        query: "semantic backend target function".to_string(),
        backend: Some(SearchBackend::Hybrid),
        limit: 1,
        offset: 0,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let explicit_execution = explicit
        .execution
        .as_ref()
        .expect("explicit backend should return execution");
    assert_eq!(explicit_execution.trace.strategy_id, "fast_search_hybrid");
    assert_eq!(handler.ensure_embedding_provider_call_count(), 1);

    Ok(())
}

#[tokio::test]
async fn off_semantics_with_explicit_backends_never_waits_for_provider() -> Result<()> {
    for backend in [SearchBackend::Semantic, SearchBackend::Hybrid] {
        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path();
        fs::create_dir_all(workspace_path.join("src"))?;
        fs::write(
            workspace_path.join("src/lib.rs"),
            "pub fn off_backend_marker() {}\n",
        )?;
        let handler = index_workspace(workspace_path).await?;

        let run = FastSearchTool {
            query: "off_backend_marker".to_string(),
            backend: Some(backend),
            semantics: Some(julie_core::embeddings_contract::SemanticMode::Off),
            ..Default::default()
        }
        .execute_with_trace(&handler)
        .await?;

        let execution = run.execution.expect("fast_search should return execution");
        assert!(
            execution
                .hits
                .iter()
                .any(|hit| hit.name == "off_backend_marker")
        );
        assert!(execution.trace.backend_fallback);
        assert_eq!(handler.ensure_embedding_provider_call_count(), 0);
    }

    Ok(())
}

#[tokio::test]
async fn auto_scoped_search_returns_matching_document_with_semantic_candidates() -> Result<()> {
    let (_temp_dir, handler) = scoped_content_workspace().await?;

    for (query, expected_path) in [
        (
            "workspace routing keeps each checkout",
            "materials/routing.md",
        ),
        (
            "workspace routing keeps configuration isolated",
            "materials/routing.json",
        ),
        (
            "workspace routing source comment describes checkout isolation",
            "materials/routing.rs",
        ),
    ] {
        let run = FastSearchTool {
            query: query.to_string(),
            file_pattern: Some("materials/**".to_string()),
            limit: 2,
            ..Default::default()
        }
        .execute_with_trace(&handler)
        .await?;

        let execution = run.execution.expect("fast_search should return execution");
        if expected_path.ends_with(".md") {
            assert!(execution.hits[0].file.ends_with(".md"), "query={query}");
        } else {
            assert_eq!(execution.hits[0].file, expected_path, "query={query}");
        }
        assert!(
            execution
                .hits
                .iter()
                .any(|hit| hit.name == "workspace_routing_candidate"),
            "query={query}"
        );
    }

    Ok(())
}

#[tokio::test]
async fn auto_natural_language_code_query_keeps_semantic_primary() -> Result<()> {
    let (_temp_dir, handler) = scoped_content_workspace().await?;

    let run = FastSearchTool {
        query: "where is workspace routing kept isolated".to_string(),
        limit: 1,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run.execution.expect("fast_search should return execution");
    assert_eq!(execution.trace.strategy_id, "fast_search_semantic");
    assert_eq!(execution.hits[0].name, "workspace_routing_candidate");

    Ok(())
}

#[tokio::test]
async fn content_route_preserves_all_requested_filters() -> Result<()> {
    let (_temp_dir, handler) = scoped_content_workspace().await?;

    let run = FastSearchTool {
        query: "workspace routing source comment".to_string(),
        file_pattern: Some("materials/**".to_string()),
        language: Some("rust".to_string()),
        exclude_tests: Some(true),
        limit: 5,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run.execution.expect("fast_search should return execution");
    assert!(
        execution
            .hits
            .iter()
            .any(|hit| hit.file == "materials/routing.rs" && hit.as_symbol().is_none())
    );
    assert!(execution.hits.iter().all(|hit| {
        hit.file.starts_with("materials/")
            && hit.language == "rust"
            && !hit.file.contains("routing_test.rs")
    }));

    Ok(())
}

#[tokio::test]
async fn content_route_never_rescues_outside_file_pattern() -> Result<()> {
    let (_temp_dir, handler) = scoped_content_workspace().await?;

    let run = FastSearchTool {
        query: "workspace routing outside requested scope".to_string(),
        file_pattern: Some("missing/**".to_string()),
        limit: 5,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run.execution.expect("fast_search should return execution");
    assert!(execution.hits.is_empty());
    assert!(!execution.trace.scope_relaxed);
    assert!(!extract_text(&run.result).contains("outside/routing.md"));

    Ok(())
}

#[tokio::test]
async fn scoped_auto_content_paging_keeps_all_line_matches() -> Result<()> {
    let (_temp_dir, handler) = scoped_content_workspace().await?;
    let mut files = Vec::new();

    for offset in 0..3 {
        let run = FastSearchTool {
            query: "workspace routing keeps each checkout".to_string(),
            file_pattern: Some("materials/**".to_string()),
            limit: 1,
            offset,
            ..Default::default()
        }
        .execute_with_trace(&handler)
        .await?;
        files.extend(
            run.execution
                .expect("fast_search should return execution")
                .hits
                .into_iter()
                .map(|hit| hit.file),
        );
    }

    assert!(
        files.contains(&"materials/routing.md".to_string()),
        "{files:?}"
    );
    assert!(
        files.contains(&"materials/routing-copy.md".to_string()),
        "{files:?}"
    );

    Ok(())
}

#[tokio::test]
async fn scoped_auto_compact_output_labels_mixed_content() -> Result<()> {
    let (_temp_dir, handler) = scoped_content_workspace().await?;

    let run = FastSearchTool {
        query: "workspace routing keeps each checkout".to_string(),
        file_pattern: Some("materials/**".to_string()),
        return_format: "compact".to_string(),
        limit: 2,
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let text = extract_text(&run.result);
    assert!(text.contains("(semantic+content)"), "{text}");
    assert!(text.contains("materials/routing"), "{text}");
    let structured = run
        .result
        .structured_content
        .as_ref()
        .expect("fast_search should expose structured content");
    assert_eq!(structured["backend"], "semantic");
    assert!(structured["requested_backend"].is_null());
    assert_eq!(structured["backend_auto"], true);
    assert_eq!(structured["content_enriched"], true);
    assert_eq!(structured["hits"].as_array().map(Vec::len), Some(2));
    assert_eq!(structured["hits"][0]["file"], "materials/routing-copy.md");
    assert_eq!(structured["trace"]["line_enrichment_status"], "applied");

    Ok(())
}

#[tokio::test]
async fn structured_content_reports_actual_lexical_backend_when_semantics_off() -> Result<()> {
    let (_temp_dir, handler) = semantic_workspace_with_embeddings().await?;

    let run = FastSearchTool {
        query: "semantic_backend_target".to_string(),
        backend: Some(SearchBackend::Semantic),
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Off),
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let structured = run
        .result
        .structured_content
        .expect("fast_search should expose structured content");
    assert_eq!(structured["backend"], "lexical");
    assert_eq!(structured["requested_backend"], "semantic");
    assert_eq!(structured["backend_auto"], false);

    Ok(())
}
