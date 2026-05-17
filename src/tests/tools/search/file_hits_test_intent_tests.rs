//! RED tests for the file-target sort's test-intent override.
//!
//! When the query has test intent (tokens like `test`, `tests`, `spec`,
//! `fixture`, `conftest`, or `test_*`/`*_test` shapes), `sort_file_hits`
//! must rank test paths above source paths. The eros benchmark caught this:
//! julie scored 0/16 on test-intent file lookups because the default
//! file_path_priority_bucket puts test paths in bucket 1 and source in
//! bucket 0.

use crate::search::index::{FileMatchKind, FileSearchResult};
use crate::tools::search::execution::sort_file_hits;
use crate::tools::search::trace::SearchHit;

fn make_file_hit(file_path: &str, match_kind: FileMatchKind, score: f32) -> SearchHit {
    let result = FileSearchResult {
        file_path: file_path.to_string(),
        language: "python".to_string(),
        score,
        match_kind,
    };
    SearchHit::from_file_result(result, "ws_test".to_string())
}

#[test]
fn test_test_intent_ranks_test_path_above_source() {
    // Query: "test synthetic extraction eval writes report artifact"
    // Both files match the same number of path tokens (PathFragment).
    // Without the override: source ranks first because bucket 0 < bucket 1.
    // With the override: test must rank first because bucket 0 < bucket 1
    // is INVERTED for test-intent queries.
    let mut hits = vec![
        make_file_hit("python/eros/eval/extraction.py", FileMatchKind::PathFragment, 1.0),
        make_file_hit(
            "tests/benchmark/test_extraction_eval.py",
            FileMatchKind::PathFragment,
            1.0,
        ),
    ];

    sort_file_hits(&mut hits, /* test_intent */ true);

    assert_eq!(
        hits[0].file, "tests/benchmark/test_extraction_eval.py",
        "test-intent query must rank test paths above source paths in file search; got order: {:?}",
        hits.iter().map(|h| &h.file).collect::<Vec<_>>()
    );
}

#[test]
fn test_non_test_intent_still_ranks_source_above_test() {
    // Regression guard: queries without test intent must keep the default
    // ordering (source above test).
    let mut hits = vec![
        make_file_hit("tests/some_test.py", FileMatchKind::PathFragment, 1.0),
        make_file_hit("src/some_module.py", FileMatchKind::PathFragment, 1.0),
    ];

    sort_file_hits(&mut hits, /* test_intent */ false);

    assert_eq!(
        hits[0].file, "src/some_module.py",
        "non-test-intent query must rank source above test (default behavior)"
    );
}

#[test]
fn test_test_intent_still_demotes_docs_and_fixtures() {
    // Even on test-intent queries, docs and fixtures should rank below
    // test/source. The override only swaps test↔source, not test↔docs.
    let mut hits = vec![
        make_file_hit("docs/testing.md", FileMatchKind::PathFragment, 1.0),
        make_file_hit("fixtures/sample_test.py", FileMatchKind::PathFragment, 1.0),
        make_file_hit("src/module.py", FileMatchKind::PathFragment, 1.0),
        make_file_hit("tests/test_module.py", FileMatchKind::PathFragment, 1.0),
    ];

    sort_file_hits(&mut hits, /* test_intent */ true);

    // Expected order: test (0), source (1), docs (2), fixtures (3)
    assert_eq!(hits[0].file, "tests/test_module.py", "test ranks #1");
    assert_eq!(hits[1].file, "src/module.py", "source ranks #2");
    let last_two: Vec<_> = hits.iter().skip(2).map(|h| h.file.as_str()).collect();
    assert!(
        last_two.contains(&"docs/testing.md"),
        "docs must be ranked below source/test"
    );
    assert!(
        last_two.contains(&"fixtures/sample_test.py"),
        "fixtures must be ranked below source/test"
    );
}

#[test]
fn test_test_intent_preserves_match_kind_priority() {
    // Match-kind is the PRIMARY sort key; test-intent only affects the
    // secondary bucket sort. An ExactBasename source file must still beat
    // a PathFragment test file even on test-intent queries.
    let mut hits = vec![
        make_file_hit("src/module.py", FileMatchKind::ExactBasename, 1.0),
        make_file_hit("tests/test_module.py", FileMatchKind::PathFragment, 1.0),
    ];

    sort_file_hits(&mut hits, /* test_intent */ true);

    assert_eq!(
        hits[0].file, "src/module.py",
        "match-kind (ExactBasename) must beat test-intent bucket swap"
    );
}
