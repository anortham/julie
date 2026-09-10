use julie_core::embeddings_contract::{EmbeddingProvider, SemanticMode};
use julie_index::search::hybrid::hybrid_search_with_tagged_embedding;
use julie_index::search::index::SearchFilter;
use julie_index::search::weights::SearchWeightProfile;
use julie_test_support::SnapshotFixture;
use tempfile::TempDir;

use super::support::{DIMS, StaticProvider, process_data_fixture, tagged};

fn fixture_with_embeddings() -> (TempDir, SnapshotFixture) {
    let (tree, fixture) = process_data_fixture();
    fixture
        .store_named_vectors(
            &StaticProvider.encoder_identity().unwrap(),
            &[("process_data", vec![0.95_f32; DIMS])],
        )
        .unwrap();
    (tree, fixture)
}

#[test]
fn test_hybrid_search_with_weight_profile_uses_weighted_merge() {
    let (_tree, fixture) = fixture_with_embeddings();

    let results = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        tagged(&StaticProvider, "process_data", SemanticMode::Auto).unwrap(),
        Some(SearchWeightProfile::fast_search()),
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(
        !results.results.is_empty(),
        "should return results with weight profile"
    );
    assert_eq!(results.results[0].name, "process_data");
}

#[test]
fn test_hybrid_search_none_profile_uses_uniform_merge() {
    let (_tree, fixture) = fixture_with_embeddings();

    let results = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        tagged(&StaticProvider, "process_data", SemanticMode::Auto).unwrap(),
        None,
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(
        !results.results.is_empty(),
        "should return results with None profile"
    );
}

#[test]
fn test_hybrid_search_weight_profile_keyword_only_graceful() {
    let (_tree, fixture) = process_data_fixture();

    let results = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        None,
        Some(SearchWeightProfile::get_context()),
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(
        !results.results.is_empty(),
        "keyword-only should still work with a weight profile"
    );
}
