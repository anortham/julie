use super::make_result;

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

/// Mixed queries that pair an identifier name with prose context — the most
/// common dogfood pattern (e.g. "workspace get_reference_scores",
/// "how does fast_refs find callers") — are NL-shaped and MUST engage the
/// path prior so docs/plans don't outrank actual production code definitions.
///
/// Prior to 2026-05-17 this was vetoed because `is_nl_like_query` rejected
/// any query containing a single identifier-like term; that silently disabled
/// hybrid + reranker for the queries users care about most.
#[test]
fn test_mixed_nl_and_identifier_query_applies_path_prior() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "workspace get_reference_scores");

    let docs_after = results
        .iter()
        .find(|r| r.id == "docs")
        .expect("docs result present")
        .score;
    let src_after = results
        .iter()
        .find(|r| r.id == "src")
        .expect("src result present")
        .score;

    assert!(
        docs_after < 2.0,
        "docs/** score should be penalized by NL path prior, got {docs_after}"
    );
    assert!(
        src_after > 1.0,
        "src/** score should be boosted by NL path prior, got {src_after}"
    );
}

/// Regression guard: queries where EVERY term is identifier-like are still a
/// pure multi-symbol lookup and must stay off the NL path. The path prior
/// must remain a no-op so exact symbol searches aren't perturbed.
#[test]
fn test_all_identifier_terms_query_does_not_apply_path_prior() {
    let mut results = vec![
        make_result("docs", "docs/SEARCH_FLOW.md", 2.0),
        make_result("src", "src/tools/search/index.rs", 1.0),
    ];

    let before = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    crate::search::scoring::apply_nl_path_prior(&mut results, "extract_identifiers rrf_merge");

    let after = results
        .iter()
        .map(|r| (r.id.clone(), r.score))
        .collect::<Vec<_>>();

    assert_eq!(
        after, before,
        "all-identifier query should not trigger path prior"
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
