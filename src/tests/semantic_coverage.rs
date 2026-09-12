use anyhow::Result;
use julie_core::workspace::mutation_gate::Registry;
use julie_facts::rows::VectorRow;
use julie_index::checkout_store::{CheckoutStore, PathChange};
use julie_index::vectors::encoder_row;

use crate::embeddings::{DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity};
use crate::request_engine::semantic::{SemanticMode, SemanticReadiness};
use crate::request_engine::semantic_store::check_facts_vectors;
use crate::request_engine::{
    BindingResolver, RequestContext, RequestEngine, RequestOrigin, RuntimeFactory, ToolRequest,
};

struct Provider(&'static str);

impl EmbeddingProvider for Provider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        Ok(vec![0.1; 4])
    }

    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1; 4]; texts.len()])
    }

    fn dimensions(&self) -> usize {
        4
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock(self.0, 4))
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "test".to_string(),
            device: "cpu".to_string(),
            model_name: self.0.to_string(),
            dimensions: 4,
        }
    }
}

fn store(paths: &[(&str, &[u8], &str)]) -> (tempfile::TempDir, CheckoutStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
    let changes: Vec<_> = paths
        .iter()
        .map(|(path, bytes, language)| PathChange::Upsert {
            path: (*path).to_string(),
            bytes: bytes.to_vec(),
            language: (*language).to_string(),
        })
        .collect();
    let guard = Registry::new().try_acquire("semantic-coverage").unwrap();
    store.apply(&changes, &guard).unwrap();
    (dir, store)
}

fn store_first_vector(store: &CheckoutStore, provider: &Provider) {
    let identity = provider.encoder_identity().unwrap();
    let encoder = encoder_row(&identity).unwrap();
    let snapshot = store.current();
    let id = julie_pipeline::embeddings::pipeline::select_eligible_symbol_ids(
        snapshot.graph(),
        Some(&crate::search::language_config::LanguageConfigs::load_embedded()),
    )[0];
    let symbol = snapshot.graph().symbol(id);
    store.set_encoder(&encoder).unwrap();
    store
        .store_vectors(
            &encoder.id,
            &[VectorRow {
                blob_hash: symbol.blob_hash.clone(),
                symbol_ordinal: symbol.ordinal,
                vector: vec![0.1; 4],
            }],
        )
        .unwrap();
    store.publish_vectors().unwrap();
}

#[test]
fn required_semantics_refuses_partial_vector_generation() {
    let (_dir, store) = store(&[(
        "src/lib.rs",
        b"pub fn alpha() {}\npub fn beta() {}\n",
        "rust",
    )]);
    let provider = Provider("coverage-model");
    store_first_vector(&store, &provider);
    let snapshot = store.current();
    let configs = crate::search::language_config::LanguageConfigs::load_embedded();

    let error = check_facts_vectors(
        snapshot.as_ref(),
        &configs,
        &provider,
        SemanticMode::Required,
    )
    .unwrap_err();
    assert_eq!(error.code, "SEMANTICS_NOT_READY");
    assert_eq!(error.details["coverage"], "partial");
    assert_eq!(error.details["embedded_symbols"], 1);
    assert_eq!(error.details["eligible_symbols"], 2);

    let auto =
        check_facts_vectors(snapshot.as_ref(), &configs, &provider, SemanticMode::Auto).unwrap();
    assert!(
        matches!(auto, SemanticReadiness::Degraded { ref reason, eligible_symbols: Some(2), embedded_symbols: Some(1), .. } if reason == "VECTORS_PARTIAL")
    );
    assert_eq!(
        auto.to_request_readiness(SemanticMode::Auto)
            .coverage
            .as_deref(),
        Some("1/2")
    );
}

#[test]
fn coverage_ignores_ineligible_symbols_and_incompatible_vectors() {
    let (_dir, store) = store(&[
        ("src/lib.rs", b"pub fn alpha() {}\n", "rust"),
        ("tests/check.rs", b"pub fn test_helper() {}\n", "rust"),
        ("README.md", b"# Architecture\n", "markdown"),
    ]);
    let provider = Provider("coverage-model");
    store_first_vector(&store, &provider);
    let snapshot = store.current();
    let configs = crate::search::language_config::LanguageConfigs::load_embedded();

    let ready = check_facts_vectors(
        snapshot.as_ref(),
        &configs,
        &provider,
        SemanticMode::Required,
    )
    .unwrap();
    assert!(matches!(
        ready,
        SemanticReadiness::Ready {
            eligible_symbols: 1,
            embedded_symbols: 1,
            ..
        }
    ));

    let error = check_facts_vectors(
        snapshot.as_ref(),
        &configs,
        &Provider("different-model"),
        SemanticMode::Required,
    )
    .unwrap_err();
    assert_eq!(error.details["coverage"], "incompatible");
}

#[test]
fn coverage_deduplicates_shared_blob_symbol_keys() {
    let source = b"pub fn shared() {}\n";
    let (_dir, store) = store(&[
        ("a_src/lib.rs", source, "rust"),
        ("z_tests/check.rs", source, "rust"),
    ]);
    let provider = Provider("coverage-model");
    store_first_vector(&store, &provider);
    let snapshot = store.current();

    let readiness = check_facts_vectors(
        snapshot.as_ref(),
        &crate::search::language_config::LanguageConfigs::load_embedded(),
        &provider,
        SemanticMode::Required,
    )
    .unwrap();
    assert!(matches!(
        readiness,
        SemanticReadiness::Ready {
            eligible_symbols: 1,
            embedded_symbols: 1,
            ..
        }
    ));
}

#[test]
fn coverage_tracks_empty_ineligible_edits_and_deletions() {
    let (_dir, store) = store(&[("README.md", b"# Notes\n", "markdown")]);
    let provider = Provider("coverage-model");
    let configs = crate::search::language_config::LanguageConfigs::load_embedded();
    let empty = check_facts_vectors(
        store.current().as_ref(),
        &configs,
        &provider,
        SemanticMode::Required,
    )
    .unwrap();
    assert!(matches!(
        empty,
        SemanticReadiness::Ready {
            eligible_symbols: 0,
            embedded_symbols: 0,
            ..
        }
    ));

    let guard = Registry::new().try_acquire("semantic-edit").unwrap();
    store
        .apply(
            &[PathChange::Upsert {
                path: "src/lib.rs".to_string(),
                bytes: b"pub fn alpha() {}\n".to_vec(),
                language: "rust".to_string(),
            }],
            &guard,
        )
        .unwrap();
    store_first_vector(&store, &provider);
    store
        .apply(
            &[PathChange::Upsert {
                path: "src/lib.rs".to_string(),
                bytes: b"pub fn beta() {}\n".to_vec(),
                language: "rust".to_string(),
            }],
            &guard,
        )
        .unwrap();
    let stale = check_facts_vectors(
        store.current().as_ref(),
        &configs,
        &provider,
        SemanticMode::Required,
    )
    .unwrap_err();
    assert_eq!(stale.details["coverage"], "missing");

    store
        .apply(
            &[PathChange::Remove {
                path: "src/lib.rs".to_string(),
            }],
            &guard,
        )
        .unwrap();
    let deleted = check_facts_vectors(
        store.current().as_ref(),
        &configs,
        &provider,
        SemanticMode::Required,
    )
    .unwrap();
    assert!(matches!(
        deleted,
        SemanticReadiness::Ready {
            eligible_symbols: 0,
            embedded_symbols: 0,
            ..
        }
    ));
}

#[tokio::test]
async fn request_after_late_provider_readiness_resumes_missing_embeddings() {
    use std::sync::Arc;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    let temp_repo = tempfile::tempdir().unwrap();
    let root = temp_repo.path().join("late-provider");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .unwrap();
    let temp_home = tempfile::tempdir().unwrap();
    let paths = crate::paths::RegistryPaths::with_home(temp_home.path().to_path_buf());
    let bindings = BindingResolver::new(Some(root.clone()), false, paths.clone());
    let runtimes = Arc::new(RuntimeFactory::new(paths));
    let semantics = Arc::new(crate::request_engine::semantic::DefaultSemanticRuntime::new(None));
    let engine =
        RequestEngine::with_semantic_runtime(bindings, runtimes.clone(), semantics.clone());
    let context = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(30)),
        CancellationToken::new(),
    );
    let binding = engine.bindings.resolve(None, None, false).unwrap().unwrap();
    let runtime = runtimes.acquire(Some(&binding), &context).await.unwrap();
    let store = runtime
        .handler()
        .checkout_store_for_workspace(&binding.workspace_id, &root)
        .await
        .unwrap();
    assert_eq!(store.status().vector_count, 0);

    semantics
        .update_provider(Some(Arc::new(Provider("late-model"))))
        .await;
    let reply = engine
        .execute(
            ToolRequest::new(
                "get_context",
                serde_json::json!({ "query": "alpha" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            RequestContext::new(
                RequestOrigin::Cli,
                Some(Duration::from_secs(30)),
                CancellationToken::new(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(reply.readiness.coverage.as_deref(), Some("0/2"));

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if !runtime
                .handler()
                .embedding_tasks
                .lock()
                .await
                .contains_key(&binding.workspace_id)
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(store.status().vector_count, 2);

    let reply = engine
        .execute(
            ToolRequest::new(
                "get_context",
                serde_json::json!({ "query": "alpha" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            RequestContext::new(
                RequestOrigin::Cli,
                Some(Duration::from_secs(30)),
                CancellationToken::new(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(reply.readiness.coverage.as_deref(), Some("full"));
}
