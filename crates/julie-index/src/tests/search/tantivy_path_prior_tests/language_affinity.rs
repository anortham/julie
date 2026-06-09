use super::make_result_with_language;

// ===========================================================================
// Language affinity prior — soft penalty for non-dominant language candidates
// on NL queries. Fixes the cross-language leakage observed in dogfood
// (Python tests ranking #1 for Rust-targeted NL queries on this codebase).
// ===========================================================================

#[test]
fn test_language_affinity_penalizes_non_dominant_language_on_nl_query() {
    let mut results = vec![
        make_result_with_language("rust1", "src/foo.rs", 10.0, "rust"),
        make_result_with_language("py1", "python/embeddings/sidecar.py", 10.0, "python"),
        make_result_with_language("rust2", "src/bar.rs", 9.5, "rust"),
    ];

    crate::search::scoring::apply_language_affinity_prior(
        &mut results,
        Some("rust"),
        "how does this thing work",
    );

    // Rust candidates keep their score; python gets 0.85x.
    let scores: std::collections::HashMap<_, _> =
        results.iter().map(|r| (r.id.clone(), r.score)).collect();
    assert_eq!(
        scores["rust1"], 10.0,
        "dominant-language candidate untouched"
    );
    assert!(
        (scores["py1"] - 10.0 * 0.85).abs() < 1e-5,
        "non-dominant candidate multiplied by NL_LANGUAGE_AFFINITY_PENALTY (0.85); got {}",
        scores["py1"]
    );

    // And the foreign-language candidate must sort BELOW both rust ones.
    assert!(
        results.iter().take(2).all(|r| r.language == "rust"),
        "rust candidates must rank above python on rust-dominant workspace; got: {:?}",
        results.iter().map(|r| &r.id).collect::<Vec<_>>()
    );
}

#[test]
fn test_language_affinity_noop_when_no_dominant_language() {
    let mut results = vec![
        make_result_with_language("rust1", "src/foo.rs", 10.0, "rust"),
        make_result_with_language("py1", "python/sidecar.py", 10.0, "python"),
    ];
    let original_scores: Vec<f32> = results.iter().map(|r| r.score).collect();

    crate::search::scoring::apply_language_affinity_prior(
        &mut results,
        None,
        "how does authentication work",
    );

    let new_scores: Vec<f32> = results.iter().map(|r| r.score).collect();
    assert_eq!(
        original_scores, new_scores,
        "mixed-language workspace (no dominant) must leave scores untouched"
    );
}

#[test]
fn test_language_affinity_noop_on_identifier_query() {
    // Identifier queries are exact-symbol lookups; do not perturb them with
    // language priors. Rust user explicitly searching for "SidecarConfig"
    // (a Python symbol) must still get it at full score.
    let mut results = vec![
        make_result_with_language("py1", "python/sidecar.py", 10.0, "python"),
        make_result_with_language("rust1", "src/foo.rs", 5.0, "rust"),
    ];

    crate::search::scoring::apply_language_affinity_prior(
        &mut results,
        Some("rust"),
        "SidecarConfig",
    );

    assert_eq!(results[0].id, "py1", "identifier-query must skip the prior");
    assert_eq!(results[0].score, 10.0);
    assert_eq!(results[1].score, 5.0);
}

#[test]
fn test_language_affinity_runs_after_path_prior_in_pipeline_order() {
    // Realistic pipeline check: reranker → path prior → language affinity.
    // A Python TEST file should be penalized BOTH for being in a test path
    // (~0.85x) AND for being a non-dominant language (~0.85x), yielding
    // ~0.72x of the original. A Rust src file gets a 1.08x boost.
    let mut results = vec![
        make_result_with_language(
            "py_test",
            "python/embeddings_sidecar/tests/test_protocol.py",
            10.0,
            "python",
        ),
        make_result_with_language("rust_src", "src/registry/lifecycle.rs", 10.0, "rust"),
    ];

    crate::search::scoring::apply_nl_path_prior(&mut results, "where is the version gate");
    crate::search::scoring::apply_language_affinity_prior(
        &mut results,
        Some("rust"),
        "where is the version gate",
    );

    let scores: std::collections::HashMap<_, _> =
        results.iter().map(|r| (r.id.clone(), r.score)).collect();

    // Rust src: 10.0 * 1.08 = 10.8 (NL_PATH_BOOST_SRC)
    assert!(
        (scores["rust_src"] - 10.0 * 1.08).abs() < 1e-4,
        "rust src should get NL_PATH_BOOST_SRC; got {}",
        scores["rust_src"]
    );
    // Python test: 10.0 * 0.85 (test path) * 0.85 (foreign lang) = 7.225
    assert!(
        (scores["py_test"] - 10.0 * 0.85 * 0.85).abs() < 1e-4,
        "python test should get both penalties stacked; got {}",
        scores["py_test"]
    );
    // And after both layers, rust_src must be #1.
    assert_eq!(results[0].id, "rust_src");
}
