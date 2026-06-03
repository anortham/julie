use super::make_result;

#[test]
fn test_fixture_penalty_is_meaningful() {
    use crate::search::scoring::{NL_PATH_BOOST_SRC, NL_PATH_PENALTY_FIXTURES};

    // Fixture penalty should be at least 20% reduction (≤ 0.80)
    assert!(
        NL_PATH_PENALTY_FIXTURES <= 0.80,
        "fixture penalty {} is too gentle — should be ≤ 0.80 to suppress noise vs source boost {}",
        NL_PATH_PENALTY_FIXTURES,
        NL_PATH_BOOST_SRC
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Original tests
// ──────────────────────────────────────────────────────────────────────────────

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
