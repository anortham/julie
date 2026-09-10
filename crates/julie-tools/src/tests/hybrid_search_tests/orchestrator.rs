use julie_core::embeddings_contract::{EmbeddingProvider, SemanticMode, TaggedQueryEmbedding};
use julie_index::search::hybrid::hybrid_search_with_tagged_embedding;
use julie_index::search::index::SearchFilter;

use super::support::{
    AxisProvider, DIMS, FailingProvider, PROCESS_DATA_SOURCE, SidecarTimeoutProvider,
    StaticProvider, axis, fixture, process_data_fixture, tagged,
};

#[test]
fn test_hybrid_search_none_provider_returns_keyword_results() {
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

    assert!(!results.results.is_empty(), "should find keyword results");
    assert_eq!(results.results[0].name, "process_data");
}

#[test]
fn test_hybrid_search_failing_provider_degrades_gracefully() {
    let (_tree, fixture) = process_data_fixture();
    let embedding = tagged(&FailingProvider, "process_data", SemanticMode::Auto).unwrap();
    assert!(embedding.is_none());

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

    assert!(
        !results.results.is_empty(),
        "should still return keyword results"
    );
    assert_eq!(results.results[0].name, "process_data");
}

#[test]
fn test_hybrid_search_preserves_relaxed_flag() {
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

    assert!(
        !results.relaxed,
        "relaxed flag should be preserved from tantivy"
    );
}

#[test]
fn test_hybrid_search_filters_semantic_results_by_language_and_file_pattern() {
    let (_tree, fixture) = fixture(&[
        ("src/lib.rs", PROCESS_DATA_SOURCE),
        (
            "scripts/tool.py",
            "def python_helper(data):\n    return data\n",
        ),
    ]);
    let provider = StaticProvider;
    fixture
        .store_named_vectors(
            &provider.encoder_identity().unwrap(),
            &[
                ("process_data", vec![0.95_f32; DIMS]),
                ("python_helper", vec![1.0_f32; DIMS]),
            ],
        )
        .unwrap();
    let filter = SearchFilter {
        language: Some("rust".to_string()),
        kind: None,
        file_pattern: Some("src/**/*.rs".to_string()),
        exclude_tests: false,
    };

    let results = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &filter,
        10,
        tagged(&provider, "process_data", SemanticMode::Auto).unwrap(),
        None,
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(!results.results.is_empty(), "expected at least one result");
    assert!(
        results.results.iter().all(|r| r.language == "rust"),
        "all results should respect language filter"
    );
    assert!(
        results
            .results
            .iter()
            .all(|r| r.file_path.starts_with("src/") && r.file_path.ends_with(".rs")),
        "all results should respect file_pattern filter"
    );
}

#[test]
fn test_hybrid_search_sidecar_timeout_degrades_to_keyword_results() {
    let (_tree, fixture) = process_data_fixture();
    let timeout = SidecarTimeoutProvider;
    fixture
        .store_named_vectors(
            &timeout.encoder_identity().unwrap(),
            &[("process_data", vec![1.0_f32; DIMS])],
        )
        .unwrap();

    let results = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        tagged(&timeout, "process_data", SemanticMode::Auto).unwrap(),
        None,
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(
        !results.results.is_empty(),
        "should still return keyword results"
    );
    assert_eq!(results.results[0].name, "process_data");
}

#[test]
fn test_hybrid_search_exclude_tests_filters_semantic_results() {
    let (_tree, fixture) = fixture(&[
        ("src/lib.rs", PROCESS_DATA_SOURCE),
        ("src/tests/pipeline_tests.rs", "fn test_process_data() {}\n"),
    ]);
    let provider = AxisProvider;
    fixture
        .store_named_vectors(
            &provider.encoder_identity().unwrap(),
            &[
                ("test_process_data", axis(0, 0.9)),
                ("process_data", axis(0, 0.8)),
            ],
        )
        .unwrap();
    let filter = SearchFilter {
        exclude_tests: true,
        ..Default::default()
    };

    let results = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process data",
        &filter,
        10,
        tagged(&provider, "process data", SemanticMode::Auto).unwrap(),
        None,
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(!results.results.is_empty());
    for r in &results.results {
        assert!(
            !r.file_path.contains("tests/"),
            "Test file result '{}' in '{}' should have been filtered from semantic candidates",
            r.name,
            r.file_path
        );
    }
}

#[test]
fn test_hybrid_search_exclude_tests_filters_metadata_test_semantic_results() {
    let (_tree, fixture) = fixture(&[(
        "src/lib.rs",
        "/// Processes input data.\npub fn process_data(input: &str) -> Result<()> { Ok(()) }\n\n#[test]\nfn inline_process_data_test() {}\n",
    )]);
    let provider = AxisProvider;
    fixture
        .store_named_vectors(
            &provider.encoder_identity().unwrap(),
            &[
                ("inline_process_data_test", axis(0, 0.9)),
                ("process_data", axis(0, 0.8)),
            ],
        )
        .unwrap();
    let filter = SearchFilter {
        exclude_tests: true,
        ..Default::default()
    };

    let results = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process data",
        &filter,
        10,
        tagged(&provider, "process data", SemanticMode::Auto).unwrap(),
        None,
        SemanticMode::Auto,
    )
    .unwrap();

    assert!(!results.results.is_empty());
    assert!(
        results
            .results
            .iter()
            .all(|result| result.name != "inline_process_data_test"),
        "metadata-only inline test symbol leaked through semantic results: {:?}",
        results
            .results
            .iter()
            .map(|result| (&result.id, &result.name, &result.file_path, &result.role))
            .collect::<Vec<_>>()
    );
}

fn static_key() -> String {
    StaticProvider
        .encoder_identity()
        .unwrap()
        .storage_key()
        .unwrap()
}

#[test]
fn test_hybrid_search_with_tagged_embedding_required_fails_closed_when_unready() {
    let (_tree, fixture) = process_data_fixture();
    let tagged = TaggedQueryEmbedding::new(vec![1.0_f32; DIMS], static_key(), 0);

    let result = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        Some(tagged),
        None,
        SemanticMode::Required,
    );

    match result {
        Err(e) => {
            let err_msg = e.to_string();
            assert!(
                err_msg.contains("SEMANTICS_NOT_READY"),
                "Expected SEMANTICS_NOT_READY, got: {err_msg}"
            );
        }
        Ok(_) => panic!("Required mode MUST fail closed when no vectors are published"),
    }
}

#[test]
fn test_hybrid_search_with_tagged_embedding_required_fails_closed_on_dimension_mismatch() {
    let (_tree, fixture) = process_data_fixture();
    fixture
        .store_named_vectors(
            &StaticProvider.encoder_identity().unwrap(),
            &[("process_data", vec![1.0_f32; DIMS])],
        )
        .unwrap();
    let tagged = TaggedQueryEmbedding::new(vec![1.0_f32; 128], static_key(), 0);

    let result = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        Some(tagged),
        None,
        SemanticMode::Required,
    );

    assert!(
        result.is_err(),
        "Required mode MUST fail closed when KNN search fails with dimension mismatch"
    );
}

#[test]
fn test_hybrid_search_with_tagged_embedding_auto_degrades_when_unready() {
    let (_tree, fixture) = process_data_fixture();
    let tagged = TaggedQueryEmbedding::new(vec![1.0_f32; DIMS], static_key(), 0);

    let result = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        Some(tagged),
        None,
        SemanticMode::Auto,
    );

    assert!(
        result.is_ok(),
        "Auto mode must degrade gracefully when vectors are unready"
    );
    let res = result.unwrap();
    assert_eq!(res.results[0].name, "process_data");
}

#[test]
fn test_hybrid_search_with_tagged_embedding_required_fails_closed_when_query_embedding_none() {
    let (_tree, fixture) = process_data_fixture();

    let result = hybrid_search_with_tagged_embedding(
        &fixture.snapshot(),
        "process_data",
        &SearchFilter::default(),
        10,
        None,
        None,
        SemanticMode::Required,
    );

    match result {
        Err(e) => {
            let err_msg = e.to_string();
            assert!(
                err_msg.contains("SEMANTICS_NOT_READY"),
                "Expected SEMANTICS_NOT_READY, got: {err_msg}"
            );
        }
        Ok(_) => panic!("Required mode MUST fail closed when query_embedding is None"),
    }
}
