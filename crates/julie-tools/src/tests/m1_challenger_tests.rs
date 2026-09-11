//! Empirical challenger tests for Milestone M1:
//! - Reader isolation: untagged query vectors never reach the vector scan.
//! - Cross-model mismatch: another encoder's key is the not-ready outcome.
//! - Cancellation bubbling: a cancelled budget is an error, never a lexical fallback.

use std::sync::Arc;

use anyhow::Result;
use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity, SemanticMode,
    TaggedQueryEmbedding,
};
use julie_index::search::hybrid::{
    compute_tagged_query_embedding_for_hybrid, hybrid_search_with_tagged_embedding, vector_search,
};
use julie_index::search::index::SearchFilter;
use julie_test_support::{FakeToolContext, SnapshotFixture};
use tempfile::TempDir;

use crate::get_context::GetContextTool;
use crate::navigation::FastRefsTool;
use crate::navigation::resolution::WorkspaceTarget;
use crate::search::FastSearchTool;

const DIMS: usize = 384;

struct MockTestProvider {
    model_name: String,
}

impl MockTestProvider {
    fn new(model_name: &str) -> Self {
        Self {
            model_name: model_name.to_string(),
        }
    }
}

impl EmbeddingProvider for MockTestProvider {
    fn embed_query(&self, _text: &str, budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        budget.check_budget()?;
        Ok(vec![1.0_f32; DIMS])
    }
    fn embed_batch(
        &self,
        texts: &[String],
        budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        budget.check_budget()?;
        Ok(texts.iter().map(|_| vec![1.0_f32; DIMS]).collect())
    }
    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock(&self.model_name, DIMS))
    }
    fn dimensions(&self) -> usize {
        DIMS
    }
    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "mock".into(),
            device: "cpu".into(),
            model_name: self.model_name.clone(),
            dimensions: DIMS,
        }
    }
}

fn fixture() -> (TempDir, SnapshotFixture) {
    let tree = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tree.path().join("src")).unwrap();
    std::fs::write(
        tree.path().join("src/lib.rs"),
        "pub fn semantic_only_sym() {}\n/// Keyword doc.\npub fn keyword_sym() {}\n",
    )
    .unwrap();
    let fixture = SnapshotFixture::from_tree(tree.path()).unwrap();
    (tree, fixture)
}

fn seeded(model: &str) -> (TempDir, SnapshotFixture, String) {
    let (tree, fixture) = fixture();
    let identity = EncoderIdentity::mock(model, DIMS);
    fixture
        .store_named_vectors(&identity, &[("semantic_only_sym", vec![1.0_f32; DIMS])])
        .unwrap();
    (tree, fixture, identity.storage_key().unwrap())
}

fn context(fixture: SnapshotFixture) -> FakeToolContext {
    FakeToolContext::new()
        .with_workspace_id("primary")
        .with_primary_root(fixture.root().to_path_buf())
        .with_snapshot_fixture(fixture)
}

#[test]
fn challenge_untagged_query_vectors_cannot_query_knn_in_hybrid_search() {
    let (_tree, fixture, key) = seeded("encoder-alpha");
    let emb = vec![1.0_f32; DIMS];

    let results_untagged = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "unrelated_search_term",
        &SearchFilter::default(),
        10,
        Some(TaggedQueryEmbedding::untagged(emb.clone())),
        None,
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(
        !results_untagged
            .results
            .iter()
            .any(|r| r.name == "semantic_only_sym"),
        "Untagged query embedding must NOT yield semantic KNN results"
    );

    let results_tagged = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "unrelated_search_term",
        &SearchFilter::default(),
        10,
        Some(TaggedQueryEmbedding::new(emb, key, 0)),
        None,
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(
        results_tagged
            .results
            .iter()
            .any(|r| r.name == "semantic_only_sym"),
        "Tagged query embedding matching the published encoder must execute KNN and find symbol"
    );
}

#[test]
fn challenge_untagged_and_cross_model_vectors_fail_in_similarity_reader() {
    let (_tree, fixture, key) = seeded("model-alpha");
    let snapshot = fixture.snapshot();
    let emb = vec![1.0_f32; DIMS];

    let untagged = TaggedQueryEmbedding::untagged(emb.clone());
    let auto_res = vector_search(&snapshot, &untagged, 5, SemanticMode::Auto).unwrap();
    assert!(
        auto_res.is_empty(),
        "Untagged query in Auto mode must return empty"
    );

    let req_err = vector_search(&snapshot, &untagged, 5, SemanticMode::Required).unwrap_err();
    assert!(
        req_err.to_string().contains("SEMANTICS_NOT_READY"),
        "Untagged query in Required mode must bail with SEMANTICS_NOT_READY: {req_err}"
    );

    let cross_model = TaggedQueryEmbedding::new(emb.clone(), "model-beta", 0);
    let cross_auto = vector_search(&snapshot, &cross_model, 5, SemanticMode::Auto).unwrap();
    assert!(
        cross_auto.is_empty(),
        "Cross-model query in Auto mode must return empty"
    );

    let cross_req_err =
        vector_search(&snapshot, &cross_model, 5, SemanticMode::Required).unwrap_err();
    assert!(
        cross_req_err.to_string().contains("SEMANTICS_NOT_READY")
            && cross_req_err.to_string().contains("model-beta"),
        "Cross-model query in Required mode must bail with SEMANTICS_NOT_READY for model-beta: {cross_req_err}"
    );

    let correct = TaggedQueryEmbedding::new(emb, key, 0);
    let correct_res = vector_search(&snapshot, &correct, 5, SemanticMode::Required).unwrap();
    assert_eq!(correct_res.len(), 1);
    assert_eq!(correct_res[0].name, "semantic_only_sym");
}

#[test]
fn challenge_cancellation_bubbles_up_in_compute_tagged_query_embedding() {
    let provider = MockTestProvider::new("mock-model");
    let budget = EmbeddingRequestBudget::for_testing();
    budget.cancel();

    let auto_res = compute_tagged_query_embedding_for_hybrid(
        "test query",
        Some(&provider),
        &budget,
        SemanticMode::Auto,
    );
    assert!(
        auto_res.is_err(),
        "Cancelled budget in Auto mode must return Err, not fallback to Ok(None)"
    );
    let err_msg = auto_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("embedding request cancelled"),
        "Expected cancellation error message, got: {err_msg}"
    );

    let req_res = compute_tagged_query_embedding_for_hybrid(
        "test query",
        Some(&provider),
        &budget,
        SemanticMode::Required,
    );
    assert!(req_res.is_err());
    assert!(
        req_res
            .unwrap_err()
            .to_string()
            .contains("embedding request cancelled")
    );
}

#[tokio::test]
async fn challenge_cancellation_bubbles_up_in_fast_refs() {
    let (_tree, fixture, _key) = seeded("test-mock");
    let context =
        context(fixture).with_embedding_provider(Arc::new(MockTestProvider::new("test-mock")));
    let budget = EmbeddingRequestBudget::for_testing();
    budget.cancel();

    let tool_auto = FastRefsTool {
        symbol: "nonexistent_symbol".to_string(),
        limit: 10,
        offset: 0,
        reference_kind: None,
        include_definition: false,
        workspace: Some("primary".to_string()),
        semantics: Some(SemanticMode::Auto),
    };
    let res_auto = tool_auto
        .call_tool_with_target_and_budget(&context, &WorkspaceTarget::Primary, Some(budget.clone()))
        .await;

    assert!(
        res_auto.is_err(),
        "Cancelled budget in FastRefs must bubble up error, not return Ok fallback"
    );
    let err_str = res_auto.unwrap_err().to_string();
    assert!(
        err_str.contains("cancelled"),
        "Error must mention cancellation, got: {err_str}"
    );

    let tool_req = FastRefsTool {
        symbol: "nonexistent_symbol".to_string(),
        limit: 10,
        offset: 0,
        reference_kind: None,
        include_definition: false,
        workspace: Some("primary".to_string()),
        semantics: Some(SemanticMode::Required),
    };
    let res_req = tool_req
        .call_tool_with_target_and_budget(&context, &WorkspaceTarget::Primary, Some(budget))
        .await;

    assert!(res_req.is_err());
    assert!(res_req.unwrap_err().to_string().contains("cancelled"));
}

#[tokio::test]
async fn challenge_cancellation_bubbles_up_in_get_context() {
    let (_tree, fixture) = fixture();
    let context =
        context(fixture).with_embedding_provider(Arc::new(MockTestProvider::new("test-mock")));
    let budget = EmbeddingRequestBudget::for_testing();
    budget.cancel();

    let tool_auto = GetContextTool {
        query: "process_data".to_string(),
        max_tokens: None,
        workspace: Some("primary".to_string()),
        language: None,
        file_pattern: None,
        format: None,
        edited_files: None,
        entry_symbols: None,
        stack_trace: None,
        failing_test: None,
        max_hops: None,
        prefer_tests: None,
        semantics: Some(SemanticMode::Auto),
    };
    let res_auto = tool_auto
        .call_tool_with_target_and_budget(&context, WorkspaceTarget::Primary, Some(budget.clone()))
        .await;

    assert!(
        res_auto.is_err(),
        "Cancelled budget in GetContext must bubble up error, not return Ok fallback"
    );
    let err_str = res_auto.unwrap_err().to_string();
    assert!(
        err_str.contains("cancelled"),
        "Error must mention cancellation, got: {err_str}"
    );

    let tool_req = GetContextTool {
        query: "process_data".to_string(),
        max_tokens: None,
        workspace: Some("primary".to_string()),
        language: None,
        file_pattern: None,
        format: None,
        edited_files: None,
        entry_symbols: None,
        stack_trace: None,
        failing_test: None,
        max_hops: None,
        prefer_tests: None,
        semantics: Some(SemanticMode::Required),
    };
    let res_req = tool_req
        .call_tool_with_target_and_budget(&context, WorkspaceTarget::Primary, Some(budget))
        .await;

    assert!(res_req.is_err());
    assert!(res_req.unwrap_err().to_string().contains("cancelled"));
}

#[tokio::test]
async fn challenge_search_execution_required_mode_fails_closed_when_provider_missing() {
    let (_tree, fixture) = fixture();
    let context = context(fixture);

    let tool = FastSearchTool {
        query: "process_data".to_string(),
        language: None,
        file_pattern: None,
        limit: 6,
        context_lines: None,
        exclude_tests: None,
        backend: Some(crate::search::SearchBackend::Semantic),
        workspace: Some("primary".to_string()),
        return_format: "full".to_string(),
        offset: 0,
        semantics: Some(SemanticMode::Required),
    };

    let res = tool.call_tool(&context).await;
    assert!(
        res.is_err(),
        "Required mode MUST fail closed when provider is missing"
    );
    let err_str = res.unwrap_err().to_string();
    assert!(
        err_str.contains("SEMANTICS_NOT_READY"),
        "Error must contain SEMANTICS_NOT_READY, got: {err_str}"
    );
}
