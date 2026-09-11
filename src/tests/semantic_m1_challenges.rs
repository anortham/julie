//! Empirical Challenge Tests for Milestone M1
//!
//! Stress-tests:
//! 1. Finding 10: Zero-Eligible Workspace Readiness
//!    - Workspace with symbols but eligible_symbols == 0, embedded_symbols == 0 passes Required ensure_ready.
//!    - Workspace with symbols, eligible_symbols > 0, embedded_symbols == 0 fails Required ensure_ready.
//! 2. Finding 5: Mode Propagation & Strict Fail-Closed Enforcement
//!    - FastSearch, FastRefs, and GetContext strictly enforce SemanticMode::Required
//!      and refuse to silently fall back to keyword/empty results on embedding failure.
//!    - Under SemanticMode::Auto, all three tools gracefully degrade.

use anyhow::{Result, anyhow};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::embeddings::{DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity};
use crate::paths::RegistryPaths;
use crate::request_engine::semantic::{
    CURRENT_EMBEDDING_FORMAT_VERSION, DefaultSemanticRuntime, SemanticReadiness,
    SemanticRequirement, SemanticRuntime,
};
use crate::request_engine::types::{SemanticMode, ToolRequest, WorkspaceBinding};
use crate::request_engine::{
    BindingResolver, RequestContext, RequestEngine, RequestOrigin, RuntimeFactory,
};
use crate::tests::helpers::workspace::make_isolated_workspace_root;
use crate::tools::GetContextTool;
use crate::tools::navigation::FastRefsTool;
use crate::tools::search::FastSearchParams;
use julie_core::database::FactsStore;
use rusqlite::params;

// ---------------------------------------------------------------------------
// Mock Providers
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct MockReadyProvider {
    dimensions: usize,
    model_name: String,
}

impl MockReadyProvider {
    fn new(model_name: &str, dimensions: usize) -> Self {
        Self {
            dimensions,
            model_name: model_name.to_string(),
        }
    }
}

impl EmbeddingProvider for MockReadyProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        Ok(vec![0.1_f32; self.dimensions])
    }

    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1_f32; self.dimensions]; texts.len()])
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock(&self.model_name, self.dimensions))
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "mock".to_string(),
            device: "cpu".to_string(),
            model_name: self.model_name.clone(),
            dimensions: self.dimensions,
        }
    }
}

#[derive(Debug, Default)]
struct MockFailingEmbedProvider {
    dimensions: usize,
    model_name: String,
    fail_count: AtomicUsize,
}

impl MockFailingEmbedProvider {
    fn new(model_name: &str, dimensions: usize) -> Self {
        Self {
            dimensions,
            model_name: model_name.to_string(),
            fail_count: AtomicUsize::new(0),
        }
    }
}

impl EmbeddingProvider for MockFailingEmbedProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        self.fail_count.fetch_add(1, Ordering::SeqCst);
        Err(anyhow!("EMBED_QUERY_SIMULATED_FAILURE"))
    }

    fn embed_batch(
        &self,
        _texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        self.fail_count.fetch_add(1, Ordering::SeqCst);
        Err(anyhow!("EMBED_BATCH_SIMULATED_FAILURE"))
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock(&self.model_name, self.dimensions))
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "mock".to_string(),
            device: "cpu".to_string(),
            model_name: self.model_name.clone(),
            dimensions: self.dimensions,
        }
    }
}

// ---------------------------------------------------------------------------
// Finding 10: Zero-Eligible Workspace Readiness Challenges
// ---------------------------------------------------------------------------

#[tokio::test]
async fn challenge_finding_10_zero_eligible_workspace_passes_required_readiness() {
    let temp_repo = TempDir::new().unwrap();
    let temp_home = TempDir::new().unwrap();
    let root = make_isolated_workspace_root(temp_repo.path(), "f10_zero_eligible");
    let index_root = temp_home.path().join("indexes/f10_zero_eligible");
    let db_dir = index_root.join("db");
    std::fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("symbols.db");

    let provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5", 384));
    let expected_key = provider
        .encoder_identity()
        .and_then(|id| id.storage_key())
        .unwrap();

    // 1. Workspace has symbols (1 symbol), but 0 vectors
    let mut db = FactsStore::new(&db_path).unwrap();
    let file = crate::tests::helpers::db::file_info_builder("src/lib.rs")
        .language("rust")
        .hash("hash1")
        .size(100)
        .last_modified(0)
        .last_indexed(0)
        .build();
    crate::tests::helpers::db::store_file_info_if_missing(&mut db, &file).unwrap();
    let sym = crate::tests::helpers::db::symbol_builder("sym_probe", "probe", "src/lib.rs").build();
    db.store_symbols(&[sym]).unwrap();

    let rev = db.get_latest_facts_revision_number().unwrap().unwrap_or(0);
    db.set_embedding_config(&expected_key, 384, CURRENT_EMBEDDING_FORMAT_VERSION)
        .unwrap();

    // 2. Publish generation where eligible_symbols == 0, embedded_symbols == 0
    let gen_id = db
        .begin_embedding_generation(&expected_key, rev, 384)
        .unwrap();
    db.publish_embedding_generation(gen_id, rev, 0, 0).unwrap();

    let runtime = DefaultSemanticRuntime::new(Some(provider as Arc<dyn EmbeddingProvider>));
    let binding = WorkspaceBinding {
        workspace_id: "f10_zero_eligible".to_string(),
        root,
        index_root,
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    let cancel = CancellationToken::new();

    // MUST pass readiness in SemanticMode::Required
    let res = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await
        .expect("Zero-eligible workspace with valid empty generation must pass Required readiness");

    match res {
        SemanticReadiness::Ready {
            vector_generation,
            eligible_symbols,
            embedded_symbols,
            ..
        } => {
            assert_eq!(vector_generation, Some(gen_id));
            assert_eq!(eligible_symbols, 0);
            assert_eq!(embedded_symbols, 0);
        }
        other => panic!("Expected SemanticReadiness::Ready, got {other:?}"),
    }
}

#[tokio::test]
async fn challenge_finding_10_nonzero_eligible_unembedded_workspace_fails_required_readiness() {
    let temp_repo = TempDir::new().unwrap();
    let temp_home = TempDir::new().unwrap();
    let root = make_isolated_workspace_root(temp_repo.path(), "f10_nonzero_unembedded");
    let index_root = temp_home.path().join("indexes/f10_nonzero_unembedded");
    let db_dir = index_root.join("db");
    std::fs::create_dir_all(&db_dir).unwrap();
    let db_path = db_dir.join("symbols.db");

    let provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5", 384));
    let expected_key = provider
        .encoder_identity()
        .and_then(|id| id.storage_key())
        .unwrap();

    let mut db = FactsStore::new(&db_path).unwrap();
    let file = crate::tests::helpers::db::file_info_builder("src/lib.rs")
        .language("rust")
        .hash("hash1")
        .size(100)
        .last_modified(0)
        .last_indexed(0)
        .build();
    crate::tests::helpers::db::store_file_info_if_missing(&mut db, &file).unwrap();
    let sym = crate::tests::helpers::db::symbol_builder("sym_probe", "probe", "src/lib.rs").build();
    db.store_symbols(&[sym]).unwrap();

    let rev = db.get_latest_facts_revision_number().unwrap().unwrap_or(0);
    db.set_embedding_config(&expected_key, 384, CURRENT_EMBEDDING_FORMAT_VERSION)
        .unwrap();

    // Subcase A: eligible_symbols = 5, embedded_symbols = 0, vector_count = 0
    let gen_id_a = db
        .begin_embedding_generation(&expected_key, rev, 384)
        .unwrap();
    assert!(
        db.publish_embedding_generation(gen_id_a, rev, 5, 0)
            .is_err(),
        "publish_embedding_generation must reject incomplete generation"
    );

    let runtime = DefaultSemanticRuntime::new(Some(provider.clone() as Arc<dyn EmbeddingProvider>));
    let binding = WorkspaceBinding {
        workspace_id: "f10_nonzero_unembedded".to_string(),
        root,
        index_root,
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    let cancel = CancellationToken::new();

    let err_a = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await
        .expect_err("Nonzero-eligible workspace with 0 vectors must fail Required readiness");
    assert_eq!(err_a.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_a.details["coverage"], "missing");

    // Subcase B: vector_count > 0 (store 1 vector), but eligible = 5, embedded = 1
    let gen_id_b = db
        .begin_embedding_generation(&expected_key, rev, 384)
        .unwrap();
    db.store_embeddings_for_generation(gen_id_b, &[("sym_probe".to_string(), vec![0.1_f32; 384])])
        .unwrap();
    assert!(
        db.publish_embedding_generation(gen_id_b, rev, 5, 1)
            .is_err(),
        "publish_embedding_generation must reject incomplete generation"
    );
    // Simulate legacy/unfenced ready generation with incomplete coverage to verify ensure_ready check
    db.conn
        .execute(
            "UPDATE embedding_generations SET status = 'ready', eligible_symbols = 5, embedded_symbols = 1 WHERE id = ?1",
            params![gen_id_b],
        )
        .unwrap();

    let err_b = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await
        .expect_err(
            "Nonzero-eligible workspace with embedded < eligible must fail Required readiness",
        );
    assert_eq!(err_b.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_b.details["coverage"], "incomplete");
}

// ---------------------------------------------------------------------------
// Finding 5: Mode Propagation & Strict Fail-Closed Enforcement Challenges
// ---------------------------------------------------------------------------

#[tokio::test]
async fn challenge_finding_5_fast_refs_strictly_fails_closed_in_required_mode() {
    let temp_repo = TempDir::new().unwrap();
    let root = make_isolated_workspace_root(temp_repo.path(), "f5_fast_refs");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn probe_symbol() {}\n").unwrap();

    let temp_home = TempDir::new().unwrap();
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let binding_resolver = BindingResolver::new(Some(root.clone()), false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths.clone()));

    // Use failing provider where embed_query errors
    let provider = Arc::new(MockFailingEmbedProvider::new("bge-small-en-v1.5", 384));
    let semantic_runtime = Arc::new(DefaultSemanticRuntime::new(Some(
        provider.clone() as Arc<dyn EmbeddingProvider>
    )));

    let engine = RequestEngine::with_semantic_runtime(
        binding_resolver,
        runtime_factory.clone(),
        semantic_runtime,
    );

    let dummy_ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(30)),
        CancellationToken::new(),
    );
    let binding = engine.bindings.resolve(None, None, false).unwrap();
    let runtime_handle = runtime_factory
        .acquire(binding.as_ref(), &dummy_ctx)
        .await
        .unwrap();

    // Prepare SQLite database with 1 symbol and valid ready generation
    {
        let db_path = binding.as_ref().unwrap().index_root.join("db/symbols.db");
        let mut db = FactsStore::new(&db_path).unwrap();
        let expected_key = provider
            .encoder_identity()
            .and_then(|id| id.storage_key())
            .unwrap();
        let rev = db.get_latest_facts_revision_number().unwrap().unwrap_or(0);
        db.set_embedding_config(&expected_key, 384, CURRENT_EMBEDDING_FORMAT_VERSION)
            .unwrap();
        let gen_id = db
            .begin_embedding_generation(&expected_key, rev, 384)
            .unwrap();
        db.publish_embedding_generation(gen_id, rev, 0, 0).unwrap();
    }

    let handler = runtime_handle.handler();

    // Query for a non-existent symbol with zero references (triggers semantic fallback)
    let tool_required = FastRefsTool {
        symbol: "completely_unknown_symbol_xyz".to_string(),
        include_definition: true,
        limit: 10,
        offset: 0,
        workspace: None,
        reference_kind: None,
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Required),
    };

    let budget = julie_core::embeddings_contract::EmbeddingRequestBudget::with_timeout(
        Duration::from_secs(5),
    );

    // 1. In Required mode, embedding failure MUST NOT fall back silently to empty references
    let err = handler
        .execute_fast_refs_with_budget(tool_required, budget.clone())
        .await
        .expect_err("FastRefs in Required mode must fail when semantic fallback errors");
    let err_str = err.to_string();
    assert!(
        err_str.contains("SEMANTICS_NOT_READY"),
        "Error must indicate SEMANTICS_NOT_READY, got: {err_str}"
    );

    // 2. In Auto mode, embedding failure MUST gracefully degrade to empty references
    let tool_auto = FastRefsTool {
        symbol: "completely_unknown_symbol_xyz".to_string(),
        include_definition: true,
        limit: 10,
        offset: 0,
        workspace: None,
        reference_kind: None,
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Auto),
    };
    let ok_res = handler
        .execute_fast_refs_with_budget(tool_auto, budget)
        .await
        .expect("FastRefs in Auto mode must degrade gracefully");
    let text = serde_json::to_string(&ok_res).unwrap();
    assert!(
        text.contains("No references found"),
        "Auto mode must return formatted empty references, got: {text}"
    );
}

#[tokio::test]
async fn challenge_finding_5_get_context_strictly_fails_closed_in_required_mode() {
    let temp_repo = TempDir::new().unwrap();
    let root = make_isolated_workspace_root(temp_repo.path(), "f5_get_context");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn probe_fn() {}\n").unwrap();

    let temp_home = TempDir::new().unwrap();
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let binding_resolver = BindingResolver::new(Some(root.clone()), false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths.clone()));

    // Use failing provider where embed_query errors
    let provider = Arc::new(MockFailingEmbedProvider::new("bge-small-en-v1.5", 384));
    let semantic_runtime = Arc::new(DefaultSemanticRuntime::new(Some(
        provider.clone() as Arc<dyn EmbeddingProvider>
    )));

    let engine = RequestEngine::with_semantic_runtime(
        binding_resolver,
        runtime_factory.clone(),
        semantic_runtime,
    );

    let dummy_ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(30)),
        CancellationToken::new(),
    );
    let binding = engine.bindings.resolve(None, None, false).unwrap();
    let runtime_handle = runtime_factory
        .acquire(binding.as_ref(), &dummy_ctx)
        .await
        .unwrap();

    // Prepare SQLite database with ready generation so ensure_ready passes
    {
        let db_path = binding.as_ref().unwrap().index_root.join("db/symbols.db");
        let mut db = FactsStore::new(&db_path).unwrap();
        let expected_key = provider
            .encoder_identity()
            .and_then(|id| id.storage_key())
            .unwrap();
        let rev = db.get_latest_facts_revision_number().unwrap().unwrap_or(0);
        db.set_embedding_config(&expected_key, 384, CURRENT_EMBEDDING_FORMAT_VERSION)
            .unwrap();
        let gen_id = db
            .begin_embedding_generation(&expected_key, rev, 384)
            .unwrap();
        db.publish_embedding_generation(gen_id, rev, 0, 0).unwrap();
    }

    let handler = runtime_handle.handler();

    let tool_required = GetContextTool {
        query: "probe_fn".to_string(),
        max_tokens: Some(1000),
        workspace: None,
        language: None,
        file_pattern: None,
        format: None,
        edited_files: None,
        entry_symbols: None,
        stack_trace: None,
        failing_test: None,
        max_hops: None,
        prefer_tests: None,
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Required),
    };

    let budget = julie_core::embeddings_contract::EmbeddingRequestBudget::with_timeout(
        Duration::from_secs(5),
    );

    // 1. In Required mode, embedding failure MUST NOT fall back silently to lexical context
    let err = handler
        .execute_get_context_with_budget(tool_required, budget.clone())
        .await
        .expect_err("GetContext in Required mode must fail closed when embedding fails");
    let err_str = err.to_string();
    assert!(
        err_str.contains("SEMANTICS_NOT_READY"),
        "Error must indicate SEMANTICS_NOT_READY, got: {err_str}"
    );

    // 2. In Auto mode, embedding failure MUST gracefully degrade to lexical search
    let tool_auto = GetContextTool {
        query: "probe_fn".to_string(),
        max_tokens: Some(1000),
        workspace: None,
        language: None,
        file_pattern: None,
        format: None,
        edited_files: None,
        entry_symbols: None,
        stack_trace: None,
        failing_test: None,
        max_hops: None,
        prefer_tests: None,
        semantics: Some(julie_core::embeddings_contract::SemanticMode::Auto),
    };
    let ok_res = handler
        .execute_get_context_with_budget(tool_auto, budget)
        .await
        .expect("GetContext in Auto mode must degrade gracefully");
    assert!(!ok_res.content.is_empty());
}

#[tokio::test]
async fn challenge_finding_5_fast_search_strictly_fails_closed_in_required_mode() {
    let temp_repo = TempDir::new().unwrap();
    let root = make_isolated_workspace_root(temp_repo.path(), "f5_fast_search");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn probe_search() {}\n").unwrap();

    let temp_home = TempDir::new().unwrap();
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let binding_resolver = BindingResolver::new(Some(root.clone()), false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths.clone()));

    // Use failing provider where embed_query errors
    let provider = Arc::new(MockFailingEmbedProvider::new("bge-small-en-v1.5", 384));
    let semantic_runtime = Arc::new(DefaultSemanticRuntime::new(Some(
        provider.clone() as Arc<dyn EmbeddingProvider>
    )));

    let engine = RequestEngine::with_semantic_runtime(
        binding_resolver,
        runtime_factory.clone(),
        semantic_runtime,
    );

    let dummy_ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(30)),
        CancellationToken::new(),
    );
    let binding = engine.bindings.resolve(None, None, false).unwrap();
    let runtime_handle = runtime_factory
        .acquire(binding.as_ref(), &dummy_ctx)
        .await
        .unwrap();

    // Prepare SQLite database with ready generation so ensure_ready passes
    {
        let db_path = binding.as_ref().unwrap().index_root.join("db/symbols.db");
        let mut db = FactsStore::new(&db_path).unwrap();
        let expected_key = provider
            .encoder_identity()
            .and_then(|id| id.storage_key())
            .unwrap();
        let rev = db.get_latest_facts_revision_number().unwrap().unwrap_or(0);
        db.set_embedding_config(&expected_key, 384, CURRENT_EMBEDDING_FORMAT_VERSION)
            .unwrap();
        let gen_id = db
            .begin_embedding_generation(&expected_key, rev, 384)
            .unwrap();
        db.publish_embedding_generation(gen_id, rev, 0, 0).unwrap();
    }

    let handler = runtime_handle.handler();

    // 1. Search with Semantic backend in Required mode MUST fail closed when embedding fails
    let params_required: FastSearchParams = serde_json::from_value(json!({
        "query": "probe_search",
        "backend": "semantic",
        "semantics": "required",
    }))
    .unwrap();

    let budget = julie_core::embeddings_contract::EmbeddingRequestBudget::with_timeout(
        Duration::from_secs(5),
    );

    let err = handler
        .execute_fast_search_with_budget(params_required, budget.clone())
        .await
        .expect_err(
            "FastSearch with Semantic backend in Required mode must fail closed on embed error",
        );
    let err_str = err.to_string();
    assert!(
        err_str.contains("SEMANTICS_NOT_READY")
            || err_str.contains("EMBED_QUERY_SIMULATED_FAILURE"),
        "Error must reflect semantic failure, got: {err_str}"
    );

    // 2. Verified request envelope mapping: dispatching through RequestEngine must return code == "SEMANTICS_NOT_READY"
    let tool_req = ToolRequest {
        name: "fast_search".to_string(),
        arguments: json!({
            "query": "probe_search",
            "backend": "semantic",
            "semantics": "required",
        })
        .as_object()
        .unwrap()
        .clone(),
        workspace: None,
        semantics: SemanticMode::Required,
    };
    let req_ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(5)),
        CancellationToken::new(),
    );
    let failure = engine
        .execute(tool_req, req_ctx)
        .await
        .expect_err("Dispatching through engine in Required mode must fail with RequestFailure");
    assert_eq!(
        failure.code, "SEMANTICS_NOT_READY",
        "Dispatch error code must be SEMANTICS_NOT_READY, got: {}",
        failure.code
    );

    // 3. Search with Semantic backend in Auto mode MUST degrade gracefully
    let params_auto: FastSearchParams = serde_json::from_value(json!({
        "query": "probe_search",
        "backend": "semantic",
        "semantics": "auto",
    }))
    .unwrap();

    let ok_res = handler
        .execute_fast_search_with_budget(params_auto, budget)
        .await
        .expect("FastSearch with Semantic backend in Auto mode must degrade gracefully");
    assert!(!ok_res.content.is_empty());
}
