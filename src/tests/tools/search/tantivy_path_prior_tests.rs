//! RED tests for NL-only path-prior scoring.

use crate::search::index::SymbolSearchResult;

fn make_result(id: &str, file_path: &str, score: f32) -> SymbolSearchResult {
    SymbolSearchResult {
        id: id.to_string(),
        name: format!("sym_{id}"),
        signature: String::new(),
        doc_comment: String::new(),
        file_path: file_path.to_string(),
        kind: "function".to_string(),
        language: "rust".to_string(),
        start_line: 1,
        score,
    }
}

#[test]
fn test_nl_like_query_applies_conservative_src_boost_and_non_code_penalties() {
    let mut results = vec![
        make_result("src", "src/tools/search/index.rs", 1.0),
        make_result("docs", "docs/SEARCH_FLOW.md", 1.0),
        make_result(
            "tests",
            "src/tests/tools/search/tantivy_integration_tests.rs",
            1.0,
        ),
        make_result("fixtures", "fixtures/real-world/sample.rs", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "workspace routing");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let docs = results.iter().find(|r| r.id == "docs").unwrap();
    let tests = results.iter().find(|r| r.id == "tests").unwrap();
    let fixtures = results.iter().find(|r| r.id == "fixtures").unwrap();
    let src_mult = src.score;
    let docs_mult = docs.score;
    let tests_mult = tests.score;
    let fixtures_mult = fixtures.score;

    assert!(
        src_mult >= 1.03,
        "src/** boost should be meaningfully above no-op"
    );
    assert!(src_mult <= 1.20, "src/** boost should remain conservative");

    assert!(docs_mult <= 0.97, "docs/** should receive a real penalty");
    assert!(
        tests_mult <= 0.97,
        "src/tests/** should receive a real penalty"
    );
    assert!(
        fixtures_mult <= 0.97,
        "fixtures/** should receive a real penalty"
    );

    assert!(src.score > docs.score, "src/** should outrank docs/**");
    assert!(
        src.score > tests.score,
        "src/** should outrank src/tests/**"
    );
    assert!(
        src.score > fixtures.score,
        "src/** should outrank fixtures/**"
    );

    assert!(
        src_mult / docs_mult >= 1.08,
        "src/** should beat docs/** by a non-trivial margin"
    );
    assert!(
        src_mult / tests_mult >= 1.08,
        "src/** should beat src/tests/** by a non-trivial margin"
    );
    assert!(
        src_mult / fixtures_mult >= 1.08,
        "src/** should beat fixtures/** by a non-trivial margin"
    );
}

#[test]
fn test_identifier_query_does_not_apply_path_prior() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "get_reference_scores");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(
        after, before,
        "identifier query should not trigger path prior"
    );
}

#[test]
fn test_mixed_nl_and_identifier_query_does_not_apply_path_prior() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "workspace get_reference_scores");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(
        after, before,
        "mixed NL + identifier query should not trigger path prior"
    );
}

#[test]
fn test_single_word_query_is_no_op() {
    let mut results = vec![
        make_result("src", "src/tools/search/index.rs", 1.0),
        make_result("docs", "docs/SEARCH_FLOW.md", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "workspace");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(after, before, "single-word query should be a no-op");
}

#[test]
fn test_multi_word_nl_with_numeric_token_still_applies_path_prior() {
    let mut results = vec![
        make_result("src", "src/tools/auth/token_refresh.rs", 1.0),
        make_result("docs", "docs/OAUTH2.md", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "oauth2 token refresh");

    let src = results.iter().find(|r| r.id == "src").unwrap();
    let docs = results.iter().find(|r| r.id == "docs").unwrap();

    assert!(src.score > 1.0, "multi-word NL should boost src/**");
    assert!(docs.score < 1.0, "multi-word NL should penalize docs/**");
    assert!(
        src.score / docs.score >= 1.08,
        "multi-word NL query should produce a non-trivial src/docs gap"
    );
}

#[test]
fn test_camel_case_identifier_query_does_not_apply_path_prior() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "getReferenceScores");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(
        after, before,
        "camelCase identifier query should not trigger path prior"
    );
}

#[test]
fn test_empty_query_is_no_op_and_does_not_panic() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(after, before, "empty query should be a no-op");
}

#[test]
fn test_whitespace_only_query_is_no_op_and_does_not_panic() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "   \t\n  ");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(after, before, "whitespace-only query should be a no-op");
}

#[test]
fn test_nl_path_prior_is_deterministic_for_same_inputs() {
    let baseline = vec![
        make_result("a", "src/core/workspace_router.rs", 1.0),
        make_result("b", "docs/workspace-routing.md", 1.0),
        make_result("c", "src/tests/tools/search/quality.rs", 1.0),
        make_result("d", "fixtures/real-world/router.rs", 1.0),
    ];

    let mut run_one = baseline.clone();
    let mut run_two = baseline.clone();

    crate::search::scoring::apply_nl_path_prior(&mut run_one, "workspace routing");
    crate::search::scoring::apply_nl_path_prior(&mut run_two, "workspace routing");

    let one = run_one
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();
    let two = run_two
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(
        one, two,
        "same inputs should produce identical ranking and scores"
    );
    assert!(
        run_one.windows(2).all(|w| w[0].score >= w[1].score),
        "results should remain sorted by descending score after prior"
    );
}
