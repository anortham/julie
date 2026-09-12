use anyhow::Result;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::embeddings::{DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity};
use crate::paths::RegistryPaths;
use crate::request_engine::semantic::DefaultSemanticRuntime;
use crate::request_engine::{
    BindingResolver, RequestContext, RequestEngine, RequestOrigin, RuntimeFactory, SemanticMode,
    ToolRequest,
};
use crate::tests::helpers::workspace::make_isolated_workspace_root;
use julie_facts::rows::VectorRow;

#[derive(Debug)]
struct CountingProvider {
    calls: AtomicUsize,
}

impl EmbeddingProvider for CountingProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![0.1; 384])
    }

    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        self.calls.fetch_add(texts.len(), Ordering::SeqCst);
        Ok(vec![vec![0.1; 384]; texts.len()])
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock("bge-small-en-v1.5", 384))
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "mock".to_string(),
            device: "cpu".to_string(),
            model_name: "bge-small-en-v1.5".to_string(),
            dimensions: 384,
        }
    }
}

#[tokio::test]
async fn concurrent_off_request_cannot_disable_required_request_provider() {
    let temp_repo = tempfile::tempdir().unwrap();
    let root = make_isolated_workspace_root(temp_repo.path(), "concurrent_semantics");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn concurrency_probe() {}\n").unwrap();

    let temp_home = tempfile::tempdir().unwrap();
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let bindings = BindingResolver::new(Some(root.clone()), false, registry_paths.clone());
    let runtimes = Arc::new(RuntimeFactory::new(registry_paths));
    let provider = Arc::new(CountingProvider {
        calls: AtomicUsize::new(0),
    });
    let semantic_runtime = Arc::new(DefaultSemanticRuntime::new(Some(
        provider.clone() as Arc<dyn EmbeddingProvider>
    )));
    let engine = RequestEngine::with_semantic_runtime(bindings, runtimes.clone(), semantic_runtime);

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
    let snapshot = store.current();
    let symbol_id = snapshot
        .graph()
        .find_by_name("concurrency_probe")
        .first()
        .copied()
        .unwrap();
    let symbol = snapshot.graph().symbol(symbol_id);
    let identity = provider.encoder_identity().unwrap();
    let encoder = julie_index::vectors::encoder_row(&identity).unwrap();
    store.set_encoder(&encoder).unwrap();
    store
        .store_vectors(
            &encoder.id,
            &[VectorRow {
                blob_hash: symbol.blob_hash.clone(),
                symbol_ordinal: symbol.ordinal,
                vector: vec![0.1; 384],
            }],
        )
        .unwrap();
    store.publish_vectors().unwrap();

    let reached = Arc::new(tokio::sync::Barrier::new(2));
    let release = Arc::new(tokio::sync::Barrier::new(2));
    engine.set_dispatch_barrier(SemanticMode::Required, reached.clone(), release.clone());
    let engine = Arc::new(engine);
    let required_engine = engine.clone();
    let required = tokio::spawn(async move {
        required_engine
            .execute(
                ToolRequest::new(
                    "get_context",
                    json!({ "query": "concurrency probe" })
                        .as_object()
                        .unwrap()
                        .clone(),
                )
                .with_semantics(SemanticMode::Required),
                RequestContext::new(
                    RequestOrigin::Cli,
                    Some(Duration::from_secs(10)),
                    CancellationToken::new(),
                ),
            )
            .await
    });

    tokio::time::timeout(Duration::from_secs(5), reached.wait())
        .await
        .expect("required request did not reach dispatch");
    let calls_before_off = provider.calls.load(Ordering::SeqCst);
    let off = engine
        .execute(
            ToolRequest::new(
                "fast_search",
                json!({ "query": "concurrency_probe", "backend": "lexical" })
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .with_semantics(SemanticMode::Off),
            RequestContext::new(
                RequestOrigin::Cli,
                Some(Duration::from_secs(10)),
                CancellationToken::new(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(off.readiness.status, "disabled");
    assert_eq!(provider.calls.load(Ordering::SeqCst), calls_before_off);

    tokio::time::timeout(Duration::from_secs(5), release.wait())
        .await
        .expect("required request was not released");
    let required = required.await.unwrap().unwrap();
    assert_eq!(required.readiness.status, "ready");
    assert_eq!(provider.calls.load(Ordering::SeqCst), calls_before_off + 1);
}
