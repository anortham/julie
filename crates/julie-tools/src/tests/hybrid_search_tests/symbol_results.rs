use julie_core::embeddings_contract::{EmbeddingProvider, SemanticMode, TaggedQueryEmbedding};
use julie_index::search::hybrid::{symbol_search_result, vector_search};

use super::support::{AxisProvider, PROCESS_DATA_SOURCE, axis, fixture};

fn id_of(snapshot: &julie_index::snapshot::Snapshot, name: &str) -> julie_index::graph::SymbolId {
    snapshot.graph().find_by_name(name)[0]
}

#[test]
fn test_symbol_search_result_converts_correctly() {
    let (_tree, fixture) = fixture(&[
        ("src/lib.rs", PROCESS_DATA_SOURCE),
        ("src/service.rs", "pub struct UserService;\n"),
    ]);
    let snapshot = fixture.snapshot();
    let graph = snapshot.graph();

    let first = symbol_search_result(graph, id_of(&snapshot, "process_data"), 0.9);
    let second = symbol_search_result(graph, id_of(&snapshot, "UserService"), 0.7);

    assert_eq!(first.id, graph.symbol(id_of(&snapshot, "process_data")).id);
    assert_eq!(first.name, "process_data");
    assert_eq!(first.kind, "function");
    assert_eq!(first.language, "rust");
    assert_eq!(first.file_path, "src/lib.rs");
    assert_eq!(first.start_line, 2);
    assert!(
        first
            .signature
            .contains("fn process_data(input: &str) -> Result<()>")
    );
    assert!(first.doc_comment.contains("Processes input data."));
    assert!((first.score - 0.9).abs() < 1e-5);
    assert_eq!(first.role, "source");
    assert_eq!(second.name, "UserService");
    assert_eq!(second.kind, "struct");
    assert_eq!(second.doc_comment, "");
    assert!((second.score - 0.7).abs() < 1e-5);
}

#[test]
fn test_symbol_search_result_uses_metadata_role_for_inline_tests() {
    let (_tree, fixture) = fixture(&[(
        "src/lib.rs",
        "#[test]\nfn inline_test_in_production_path() {}\n",
    )]);
    let snapshot = fixture.snapshot();

    let result = symbol_search_result(
        snapshot.graph(),
        id_of(&snapshot, "inline_test_in_production_path"),
        0.95,
    );

    assert_eq!(result.role, "test");
    assert_eq!(result.test_role, "test_case");
}

#[test]
fn test_vector_search_clamps_negative_cosine_to_zero() {
    let (_tree, fixture) = fixture(&[("src/clamp.rs", "pub fn clamp_me() {}\n")]);
    let provider = AxisProvider;
    fixture
        .store_named_vectors(
            &provider.encoder_identity().unwrap(),
            &[("clamp_me", axis(0, -1.0))],
        )
        .unwrap();
    let key = provider.encoder_identity().unwrap().storage_key().unwrap();

    let results = vector_search(
        &fixture.snapshot(),
        &TaggedQueryEmbedding::new(axis(0, 1.0), key, 0),
        5,
        SemanticMode::Auto,
    )
    .unwrap();

    assert_eq!(results.len(), 1);
    assert!(
        results[0].score >= 0.0,
        "score must be non-negative, got {}",
        results[0].score
    );
    assert_eq!(
        results[0].score, 0.0,
        "opposite vectors should clamp to score=0.0, got {}",
        results[0].score
    );
}
