use super::make_result;

// ===========================================================================
// Test-intent override — when the query is clearly looking for tests,
// apply_nl_path_prior must NOT penalize test paths (and must NOT boost
// source paths above tests). Without this override, test-intent queries
// like "test queue overflow" find production code instead of the test that
// matches the query terms verbatim. Caught by the eros benchmark comparison
// where julie scored 0/16 on test-intent lookups.
// ===========================================================================

#[test]
fn test_test_intent_query_skips_test_path_penalty() {
    // Query has explicit "test" keyword. The test-path symbol must not be
    // penalized below the source-path symbol.
    let mut results = vec![
        make_result("src", "src/eval/extraction.rs", 10.0),
        make_result("test", "src/tests/eval/test_extraction.rs", 10.0),
    ];

    crate::search::scoring::apply_nl_path_prior(
        &mut results,
        "test synthetic extraction eval writes report",
    );

    let scores: std::collections::HashMap<_, _> =
        results.iter().map(|r| (r.id.clone(), r.score)).collect();

    // Strong invariant: test path must be >= source path when query has test intent.
    assert!(
        scores["test"] >= scores["src"],
        "test-intent query must not push test-path below source-path; got test={}, src={}",
        scores["test"],
        scores["src"]
    );
}

#[test]
fn test_test_intent_query_does_not_penalize_test_with_spec_keyword() {
    let mut results = vec![
        make_result("src", "src/parser/json.rs", 10.0),
        make_result("test", "tests/parser_spec.rs", 10.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "spec parser json edge cases");

    let scores: std::collections::HashMap<_, _> =
        results.iter().map(|r| (r.id.clone(), r.score)).collect();
    assert!(
        scores["test"] >= scores["src"],
        "spec-intent query must not demote test paths"
    );
}

#[test]
fn test_test_intent_query_does_not_penalize_test_with_fixture_keyword() {
    let mut results = vec![
        make_result("src", "src/db/connection.rs", 10.0),
        make_result("test", "src/tests/db/connection_tests.rs", 10.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "fixture database connection setup");

    let scores: std::collections::HashMap<_, _> =
        results.iter().map(|r| (r.id.clone(), r.score)).collect();
    assert!(
        scores["test"] >= scores["src"],
        "fixture-intent query must not demote test paths"
    );
}

#[test]
fn test_non_test_intent_query_still_penalizes_test_paths() {
    // Regression guard: the test-intent override must not weaken the
    // normal "production code wins on NL queries about behavior" behavior.
    let mut results = vec![
        make_result("src", "src/eval/extraction.rs", 10.0),
        make_result("test", "src/tests/eval/test_extraction.rs", 10.0),
    ];

    // No test keyword — pure NL question about how the production code works.
    crate::search::scoring::apply_nl_path_prior(&mut results, "how does extraction parse symbols");

    let scores: std::collections::HashMap<_, _> =
        results.iter().map(|r| (r.id.clone(), r.score)).collect();
    assert!(
        scores["src"] > scores["test"],
        "non-test-intent NL query must still demote test paths; got src={}, test={}",
        scores["src"],
        scores["test"]
    );
}

#[test]
fn test_test_intent_detection_recognizes_test_underscore_prefix() {
    // Single-word query "test_extraction" is a test-function-name lookup
    // and is identifier-like (no spaces), so apply_nl_path_prior is a no-op
    // either way. But if someone types "test_extraction edge cases" — NL
    // shape — test intent should be detected from the underscore-prefixed
    // term.
    let mut results = vec![
        make_result("src", "src/eval/extraction.rs", 10.0),
        make_result("test", "tests/eval/extraction_tests.rs", 10.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "test_extraction edge cases");

    let scores: std::collections::HashMap<_, _> =
        results.iter().map(|r| (r.id.clone(), r.score)).collect();
    assert!(
        scores["test"] >= scores["src"],
        "test_*-prefix term should be recognized as test intent"
    );
}
