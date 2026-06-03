//! Pipeline-level tests for the NL path prior.
//!
//! Bug B1 (from docs/plans/2026-05-17-search-consolidation-plan.md): the NL
//! path prior used to be applied at two layers — once inside
//! `SearchIndex::search_symbols` (and `_relaxed`) and again inside
//! `definition_search_with_index`. The canonical owner is the assembly
//! point (`text_search.rs`); the low-level Tantivy search must not apply it.
//!
//! These tests enforce that contract:
//! 1. `search_symbols` / `_relaxed` return raw BM25 scores — no path prior
//!    baked in (`search_symbols_does_not_apply_nl_path_prior`,
//!    `search_symbols_relaxed_does_not_apply_nl_path_prior`).
//! 2. The pipeline (`definition_search_with_index`, keyword branch) applies
//!    the prior exactly once
//!    (`definition_search_pipeline_applies_nl_path_prior_after_reranker`).
//! 3. The hybrid branch never re-applies the prior — `hybrid::hybrid_search`
//!    is a pure retrieval/merge step and must not contain
//!    `apply_nl_path_prior`
//!    (`hybrid_search_does_not_apply_nl_path_prior_source_check`). This is
//!    a source-text contract test because building a hybrid pipeline with a
//!    fake embedding provider + `symbol_vectors` virtual table is more
//!    setup than the contract is worth.
//!
//! Function-level coverage of `apply_nl_path_prior` itself lives in
//! `tantivy_path_prior_tests.rs`.

use tempfile::TempDir;

use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};
use crate::search::scoring::{NL_PATH_BOOST_SRC, NL_PATH_PENALTY_TESTS};
use crate::tools::search::text_search::definition_search_with_index_for_test;

/// Two identical-content candidates differing only by path. BM25 base scores
/// match exactly; any divergence in returned scores comes from a score-shaping
/// pass after retrieval.
fn add_twin_candidates(index: &SearchIndex, src_path: &str, test_path: &str) {
    let shared_content = "handles user authentication service requests";

    for (id, file_path) in [("src-auth", src_path), ("test-auth", test_path)] {
        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                id,
                "AuthService",
                "pub struct AuthService",
                shared_content,
                "",
                file_path,
                "struct",
                "rust",
                1,
            ))
            .unwrap();
    }
    index.commit().unwrap();
}

/// **B1 contract**: `SearchIndex::search_symbols` must NOT apply the NL path
/// prior internally. Ownership lives at the assembly layer
/// (`definition_search_with_index`). Applying it at both layers caused the
/// same candidate to be re-multiplied or stomped on by the reranker pass
/// in between, depending on env state.
#[test]
fn search_symbols_does_not_apply_nl_path_prior() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();
    add_twin_candidates(&index, "src/auth.rs", "src/tests/auth_test.rs");

    let results = index
        .search_symbols("user authentication service", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert_eq!(results.len(), 2, "both candidates should match");

    let src = results
        .iter()
        .find(|r| r.file_path == "src/auth.rs")
        .expect("src/ candidate must be present");
    let tests = results
        .iter()
        .find(|r| r.file_path == "src/tests/auth_test.rs")
        .expect("src/tests/ candidate must be present");

    // Identical content → identical BM25. If the path prior is applied here,
    // src is boosted (×1.08) and tests is penalized (×0.85), so the ratio
    // drifts to ~1.27. Raw BM25 should leave them within floating-point noise.
    let ratio = src.score / tests.score;
    assert!(
        (ratio - 1.0).abs() < 0.01,
        "search_symbols must return raw BM25 scores; the NL path prior is the \
         assembly layer's responsibility. observed src/tests ratio = {ratio:.4}, \
         src.score = {src_score:.4}, tests.score = {tests_score:.4}",
        src_score = src.score,
        tests_score = tests.score,
    );
}

/// **B1 contract (mirror)**: same invariant for `search_symbols_relaxed`,
/// the OR-fallback variant. No production caller relies on the inner prior;
/// the assembly layer applies it after merge.
#[test]
fn search_symbols_relaxed_does_not_apply_nl_path_prior() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();
    add_twin_candidates(&index, "src/auth.rs", "src/tests/auth_test.rs");

    let results = index
        .search_symbols_relaxed("user authentication service", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    assert_eq!(results.len(), 2, "both candidates should match");

    let src = results
        .iter()
        .find(|r| r.file_path == "src/auth.rs")
        .expect("src/ candidate must be present");
    let tests = results
        .iter()
        .find(|r| r.file_path == "src/tests/auth_test.rs")
        .expect("src/tests/ candidate must be present");

    let ratio = src.score / tests.score;
    assert!(
        (ratio - 1.0).abs() < 0.01,
        "search_symbols_relaxed must return raw BM25 scores. \
         observed src/tests ratio = {ratio:.4}, \
         src.score = {src_score:.4}, tests.score = {tests_score:.4}",
        src_score = src.score,
        tests_score = tests.score,
    );
}

/// **B1 end-to-end**: the pipeline (`definition_search_with_index`) still
/// applies the NL path prior exactly once when `search_symbols` no longer
/// does. Reranker is enabled (default state); since the reranker overwrites
/// scores, the assembly-layer prior is the only multiplier that survives.
/// Identical-content twins land at the expected single-application ratio.
#[test]
fn definition_search_pipeline_applies_nl_path_prior_after_reranker() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();
    add_twin_candidates(&index, "src/auth.rs", "src/tests/auth_test.rs");

    // db=None disables centrality, DB rescue, and language-affinity. Reranker
    // still runs (default-on) and rewrites scores between any retrieval-layer
    // shaping and the assembly-layer prior, so this test sees ONLY the
    // assembly-layer prior — which is exactly the contract we want.
    let (symbols, _relaxed, _total) = definition_search_with_index_for_test(
        "user authentication service",
        &SearchFilter::default(),
        10,
        &index,
        None,
    )
    .unwrap();

    assert_eq!(symbols.len(), 2, "both candidates should be returned");

    let src = symbols
        .iter()
        .find(|s| s.file_path == "src/auth.rs")
        .expect("src/ candidate must be present");
    let tests = symbols
        .iter()
        .find(|s| s.file_path == "src/tests/auth_test.rs")
        .expect("src/tests/ candidate must be present");

    let src_score = src.confidence.expect("score should be set");
    let tests_score = tests.confidence.expect("score should be set");

    let observed_ratio = src_score / tests_score;
    let expected_single = NL_PATH_BOOST_SRC / NL_PATH_PENALTY_TESTS;

    // 5% of expected_single (~1.27) ≈ 0.064 → band [1.21, 1.34].
    let tolerance = 0.05 * expected_single;
    assert!(
        (observed_ratio - expected_single).abs() < tolerance,
        "Pipeline must apply NL path prior exactly once. \
         observed src/tests ratio = {observed_ratio:.4}, \
         expected single = {expected_single:.4}. \
         src.score = {src_score:.4}, tests.score = {tests_score:.4}"
    );
}

/// **B1 contract (hybrid branch, static):** `hybrid::hybrid_search` must
/// never apply the NL path prior. It's a retrieval+merge primitive; the
/// score-shaping pass belongs to the assembly layer. We pin this
/// statically rather than dynamically because building a real hybrid
/// fixture (mock `EmbeddingProvider` + populated `symbol_vectors` vec0
/// table + paired BM25 candidates) costs more than the contract is worth.
///
/// If the function moves, gets a wrapper, or a future refactor folds NL
/// shaping back inside `hybrid_search`, this test fails loudly and points
/// at the consolidation plan. Codex review 2026-05-17 flagged the dynamic
/// test gap; this is the cheap-but-strict cover.
#[test]
fn hybrid_search_does_not_apply_nl_path_prior_source_check() {
    let source = include_str!("../../../../crates/julie-index/src/search/hybrid.rs");
    assert!(
        !source.contains("apply_nl_path_prior"),
        "src/search/hybrid.rs must not invoke `apply_nl_path_prior`. \
         Ownership of the NL path prior lives in \
         `text_search::definition_search_with_index` per the B1 contract \
         (docs/plans/2026-05-17-search-consolidation-plan.md). If hybrid \
         now genuinely needs path shaping, fold it into the assembly \
         layer instead of re-introducing a second application point."
    );
}

/// **B1 contract (low-level retrieval, static):** mirrors the dynamic
/// `search_symbols_does_not_apply_nl_path_prior` tests but covers the
/// case where someone re-adds the call through a helper that the
/// scoring-ratio tests don't observe (e.g. a `pub(crate)` wrapper).
#[test]
fn search_index_does_not_apply_nl_path_prior_source_check() {
    let source = include_str!("../../../../crates/julie-index/src/search/index.rs");
    assert!(
        !source.contains("apply_nl_path_prior"),
        "src/search/index.rs must not invoke `apply_nl_path_prior`. The \
         comment at the existing call site already documents this; this \
         test is the durable guard."
    );
}
