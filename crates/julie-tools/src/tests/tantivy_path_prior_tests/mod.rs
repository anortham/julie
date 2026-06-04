//! RED tests for NL-only path-prior scoring.
// intent, language_affinity, query_shape, scoring_invariants relocated to
// crates/julie-index/src/tests/search/tantivy_path_prior_tests/

use julie_index::search::index::SymbolSearchResult;

fn make_result(id: &str, file_path: &str, score: f32) -> SymbolSearchResult {
    make_result_with_language(id, file_path, score, "rust")
}

fn make_result_with_language(
    id: &str,
    file_path: &str,
    score: f32,
    language: &str,
) -> SymbolSearchResult {
    SymbolSearchResult {
        id: id.to_string(),
        name: format!("sym_{id}"),
        signature: String::new(),
        doc_comment: String::new(),
        file_path: file_path.to_string(),
        kind: "function".to_string(),
        language: language.to_string(),
        start_line: 1,
        score,
        role: String::new(),
        test_role: String::new(),
    }
}

mod language_layouts;
mod path_classifiers;
