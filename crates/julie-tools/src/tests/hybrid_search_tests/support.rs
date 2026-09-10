use anyhow::Result;
use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity, SemanticMode,
    TaggedQueryEmbedding,
};
use julie_index::search::hybrid::compute_tagged_query_embedding_for_hybrid;
use julie_test_support::SnapshotFixture;
use tempfile::TempDir;

pub(super) const DIMS: usize = 384;

pub(super) fn axis(index: usize, value: f32) -> Vec<f32> {
    let mut vector = vec![0.0_f32; DIMS];
    vector[index] = value;
    vector
}

pub(super) struct StaticProvider;

impl EmbeddingProvider for StaticProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        Ok(vec![1.0_f32; DIMS])
    }
    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0_f32; DIMS]).collect())
    }
    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock("static-mock", DIMS))
    }
    fn dimensions(&self) -> usize {
        DIMS
    }
    fn device_info(&self) -> DeviceInfo {
        device("static-mock")
    }
}

pub(super) struct AxisProvider;

impl EmbeddingProvider for AxisProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        Ok(axis(0, 0.85))
    }
    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| axis(0, 0.85)).collect())
    }
    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock("test-provider", DIMS))
    }
    fn dimensions(&self) -> usize {
        DIMS
    }
    fn device_info(&self) -> DeviceInfo {
        device("test-provider")
    }
}

pub(super) struct FailingProvider;

impl EmbeddingProvider for FailingProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        anyhow::bail!("embedding model not loaded")
    }
    fn embed_batch(
        &self,
        _texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        anyhow::bail!("embedding model not loaded")
    }
    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock("failing-mock", DIMS))
    }
    fn dimensions(&self) -> usize {
        DIMS
    }
    fn device_info(&self) -> DeviceInfo {
        device("failing-mock")
    }
}

pub(super) struct SidecarTimeoutProvider;

impl EmbeddingProvider for SidecarTimeoutProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        anyhow::bail!("timed out waiting for sidecar response for method 'embed_query' after 50ms")
    }
    fn embed_batch(
        &self,
        _texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        anyhow::bail!("timed out waiting for sidecar response for method 'embed_batch' after 50ms")
    }
    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock("fake-sidecar-timeout", DIMS))
    }
    fn dimensions(&self) -> usize {
        DIMS
    }
    fn device_info(&self) -> DeviceInfo {
        device("fake-sidecar-timeout")
    }
}

pub(super) fn device(model: &str) -> DeviceInfo {
    DeviceInfo {
        runtime: "test".into(),
        device: "cpu".into(),
        model_name: model.into(),
        dimensions: DIMS,
    }
}

pub(super) const PROCESS_DATA_SOURCE: &str =
    "/// Processes input data.\npub fn process_data(input: &str) -> Result<()> { Ok(()) }\n";

pub(super) fn fixture(files: &[(&str, &str)]) -> (TempDir, SnapshotFixture) {
    let tree = tempfile::tempdir().unwrap();
    for (path, content) in files {
        let full = tree.path().join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
    }
    let fixture = SnapshotFixture::from_tree(tree.path()).unwrap();
    (tree, fixture)
}

pub(super) fn process_data_fixture() -> (TempDir, SnapshotFixture) {
    fixture(&[("src/lib.rs", PROCESS_DATA_SOURCE)])
}

pub(super) fn tagged(
    provider: &dyn EmbeddingProvider,
    query: &str,
    mode: SemanticMode,
) -> Result<Option<TaggedQueryEmbedding>> {
    compute_tagged_query_embedding_for_hybrid(
        query,
        Some(provider),
        &EmbeddingRequestBudget::default(),
        mode,
    )
}
