use julie_core::embeddings_contract::{EmbeddingRequestBudget, SemanticMode};
use julie_index::search::hybrid::{
    compute_query_embedding_for_hybrid, hybrid_search_with_tagged_embedding,
};
use julie_index::search::index::SearchFilter;

use super::support::{DIMS, FailingProvider, StaticProvider, process_data_fixture, tagged};

#[test]
fn test_compute_query_embedding_returns_vector() {
    let embedding = compute_query_embedding_for_hybrid(
        "find me something",
        Some(&StaticProvider),
        &EmbeddingRequestBudget::default(),
    );
    assert!(embedding.is_some(), "should return embedding from provider");
    assert_eq!(embedding.unwrap().len(), DIMS);
}

#[test]
fn test_compute_query_embedding_none_provider_returns_none() {
    let embedding = compute_query_embedding_for_hybrid(
        "find me something",
        None,
        &EmbeddingRequestBudget::default(),
    );
    assert!(embedding.is_none());
}

#[test]
fn test_compute_query_embedding_error_returns_none() {
    let embedding = compute_query_embedding_for_hybrid(
        "find me something",
        Some(&FailingProvider),
        &EmbeddingRequestBudget::default(),
    );
    assert!(
        embedding.is_none(),
        "provider error should degrade to None (keyword-only), not propagate"
    );
}

#[test]
fn test_hybrid_search_with_embedding_none_returns_keyword_results() {
    let (_tree, fixture) = process_data_fixture();

    let results = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        None,
        None,
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(!results.results.is_empty(), "should return keyword results");
    assert_eq!(results.results[0].name, "process_data");
}

#[test]
fn test_hybrid_search_with_precomputed_embedding_never_calls_the_provider() {
    let (_tree, fixture) = process_data_fixture();
    let embedding = tagged(&StaticProvider, "process_data", SemanticMode::Auto).unwrap();
    assert!(embedding.is_some());

    let results = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        embedding,
        None,
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(!results.results.is_empty(), "should return results");
    assert_eq!(results.results[0].name, "process_data");
}
