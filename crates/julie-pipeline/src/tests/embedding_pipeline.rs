use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use julie_core::workspace::mutation_gate::Registry;
use julie_index::checkout_store::{CheckoutStore, PathChange};
use tempfile::TempDir;

use crate::embeddings::pipeline::run_embedding_pipeline;
use crate::embeddings::{DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity};

struct CountingProvider {
    model: &'static str,
    dims: usize,
    short_by: usize,
    batches: AtomicUsize,
    texts: Mutex<Vec<String>>,
}

impl CountingProvider {
    fn new(model: &'static str, short_by: usize) -> Self {
        Self {
            model,
            dims: 4,
            short_by,
            batches: AtomicUsize::new(0),
            texts: Mutex::new(Vec::new()),
        }
    }
}

impl EmbeddingProvider for CountingProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        Ok(vec![0.1; self.dims])
    }

    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        self.batches.fetch_add(1, Ordering::SeqCst);
        self.texts.lock().unwrap().extend_from_slice(texts);
        let count = texts.len().saturating_sub(self.short_by);
        Ok((0..count).map(|i| vec![1.0, i as f32, 0.0, 0.5]).collect())
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock(self.model, self.dims))
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "test".to_string(),
            device: "cpu".to_string(),
            model_name: self.model.to_string(),
            dimensions: self.dims,
        }
    }
}

fn store_with_functions(count: usize) -> (TempDir, CheckoutStore) {
    let dir = tempfile::tempdir().unwrap();
    let source: String = (0..count)
        .map(|i| format!("pub fn do_work_{i}() {{}}\n"))
        .collect();
    std::fs::write(dir.path().join("lib.rs"), &source).unwrap();
    let store = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
    let guard = Registry::new().try_acquire("pipeline-test").unwrap();
    store
        .apply(
            &[PathChange::Upsert {
                path: "lib.rs".to_string(),
                bytes: source.into_bytes(),
                language: "rust".to_string(),
            }],
            &guard,
        )
        .unwrap();
    assert_eq!(
        store.current().graph().len(),
        count,
        "{}",
        dir.path().display()
    );
    (dir, store)
}

fn encoder_id(store: &CheckoutStore) -> String {
    store
        .current()
        .vectors()
        .encoder()
        .map(|e| e.id.clone())
        .unwrap_or_default()
}

#[test]
fn pipeline_stores_partial_results_on_batch_mismatch() {
    let (_dir, store) = store_with_functions(3);
    let provider = CountingProvider::new("partial-test-model", 1);

    let stats = run_embedding_pipeline(&store, &provider, None).unwrap();

    assert_eq!(stats.symbols_embedded, 2);
    assert_eq!(store.current().vectors().len(), 2);
    assert_eq!(store.status().vector_count, 2);
}

#[test]
fn pipeline_skips_symbols_already_embedded_for_the_encoder() {
    let (_dir, store) = store_with_functions(3);
    let provider = CountingProvider::new("full-test-model", 0);
    let first = run_embedding_pipeline(&store, &provider, None).unwrap();
    assert_eq!(first.symbols_embedded, 3);

    let second = run_embedding_pipeline(&store, &provider, None).unwrap();

    assert_eq!(second.symbols_embedded, 0);
    assert_eq!(second.symbols_skipped, 3);
    assert_eq!(provider.batches.load(Ordering::SeqCst), 1);
    assert_eq!(store.current().vectors().len(), 3);
}

#[test]
fn restart_resumes_only_missing_embeddings() {
    let (_dir, store) = store_with_functions(3);
    let interrupted = CountingProvider::new("resume-model", 1);
    let first = run_embedding_pipeline(&store, &interrupted, None).unwrap();
    assert_eq!(first.symbols_embedded, 2);
    let snapshot = store.current();
    let retained_id =
        crate::embeddings::pipeline::select_eligible_symbol_ids(snapshot.graph(), None)[0];
    let retained = snapshot.vectors().vector_of(retained_id).unwrap().to_vec();

    let resumed = CountingProvider::new("resume-model", 0);
    let second = run_embedding_pipeline(&store, &resumed, None).unwrap();

    assert_eq!(second.symbols_embedded, 1);
    assert_eq!(second.symbols_skipped, 2);
    assert_eq!(resumed.batches.load(Ordering::SeqCst), 1);
    assert_eq!(store.current().vectors().len(), 3);
    assert_eq!(
        store.current().vectors().vector_of(retained_id),
        Some(retained.as_slice())
    );
}

#[test]
fn pipeline_retains_container_field_enrichment() {
    let dir = tempfile::tempdir().unwrap();
    let source = b"pub struct Account {\n    pub account_id: u64,\n}\n";
    let store = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
    let guard = Registry::new().try_acquire("pipeline-enrichment").unwrap();
    store
        .apply(
            &[PathChange::Upsert {
                path: "lib.rs".to_string(),
                bytes: source.to_vec(),
                language: "rust".to_string(),
            }],
            &guard,
        )
        .unwrap();
    let provider = CountingProvider::new("enrichment-model", 0);

    run_embedding_pipeline(&store, &provider, None).unwrap();

    let texts = provider.texts.lock().unwrap();
    assert!(
        texts
            .iter()
            .any(|text| text.contains("fields: pub account_id: u64")),
        "{texts:?}"
    );
}

#[test]
fn pipeline_with_a_new_encoder_identity_replaces_every_vector() {
    let (_dir, store) = store_with_functions(2);
    let first = CountingProvider::new("model-a", 0);
    run_embedding_pipeline(&store, &first, None).unwrap();
    let first_key = encoder_id(&store);

    let second = CountingProvider::new("model-b", 0);
    let stats = run_embedding_pipeline(&store, &second, None).unwrap();

    assert_ne!(encoder_id(&store), first_key);
    assert_eq!(stats.symbols_embedded, 2);
    assert_eq!(store.current().vectors().len(), 2);
}

#[test]
fn pipeline_embeds_only_symbols_the_current_paths_hold() {
    let (dir, store) = store_with_functions(2);
    let provider = CountingProvider::new("model-a", 0);
    run_embedding_pipeline(&store, &provider, None).unwrap();
    let guard = Registry::new().try_acquire("pipeline-test").unwrap();
    store
        .apply(
            &[PathChange::Remove {
                path: "lib.rs".to_string(),
            }],
            &guard,
        )
        .unwrap();
    assert!(Path::new(&dir.path().join("lib.rs")).exists());

    let stats = run_embedding_pipeline(&store, &provider, None).unwrap();

    assert_eq!(stats.symbols_scanned, 0);
    assert_eq!(stats.symbols_embedded, 0);
    assert!(store.current().vectors().is_empty());
}

#[test]
fn coverage_deduplicates_shared_blob_symbol_keys() {
    let dir = tempfile::tempdir().unwrap();
    let source = b"pub fn shared_work() {}\n";
    let store = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
    let guard = Registry::new().try_acquire("pipeline-shared-blob").unwrap();
    store
        .apply(
            &[
                PathChange::Upsert {
                    path: "a.rs".to_string(),
                    bytes: source.to_vec(),
                    language: "rust".to_string(),
                },
                PathChange::Upsert {
                    path: "b.rs".to_string(),
                    bytes: source.to_vec(),
                    language: "rust".to_string(),
                },
            ],
            &guard,
        )
        .unwrap();
    assert_eq!(store.current().graph().len(), 2);
    let provider = CountingProvider::new("shared-blob-model", 0);

    let stats = run_embedding_pipeline(&store, &provider, None).unwrap();

    assert_eq!(stats.symbols_embedded, 1);
    assert_eq!(stats.symbols_skipped, 0);
    assert_eq!(store.current().vectors().len(), 1);
}
