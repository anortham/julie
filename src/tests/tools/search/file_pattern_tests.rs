//! Tests for multi-pattern `file_pattern` parser and boundary normalization.
//!
//! Covers `matches_glob_pattern`:
//! - Single-pattern (legacy) behaviors preserved
//! - Comma-separated OR semantics
//! - Brace alternation
//! - Exclusions via `!` prefix, including mixed include/exclude
//! - Whitespace inside a glob (literal space path) stays a single pattern
//! - Whitespace between globs is NOT a split separator

use crate::tools::search::matches_glob_pattern;

// ---------------------------------------------------------------------------
// Comma-separated OR semantics
// ---------------------------------------------------------------------------

#[test]
fn comma_splits_into_or_of_inclusions() {
    // Real-world case from the plan: overlapping patterns joined with commas.
    let path = "src/database/workspace.rs";
    let pattern = "src/database/*.rs,src/database/**/*.rs";
    assert!(
        matches_glob_pattern(path, pattern),
        "comma-separated inclusions should OR — {} should match '{}'",
        path,
        pattern,
    );
}

#[test]
fn comma_or_matches_second_alternative() {
    let pattern = "src/**,tests/**";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(matches_glob_pattern("tests/foo/bar.rs", pattern));
    assert!(
        !matches_glob_pattern("docs/README.md", pattern),
        "docs/ is not in either inclusion; should not match",
    );
}

// ---------------------------------------------------------------------------
// Brace alternation (globset native feature; must survive comma splitting)
// ---------------------------------------------------------------------------

#[test]
fn brace_alternation_is_preserved() {
    // Top-level comma split must skip commas inside `{...}`.
    let pattern = "{src/**,tests/**}";
    assert!(
        matches_glob_pattern("src/lib.rs", pattern),
        "brace alternation should match src/ tree",
    );
    assert!(
        matches_glob_pattern("tests/foo.rs", pattern),
        "brace alternation should match tests/ tree",
    );
    assert!(!matches_glob_pattern("docs/README.md", pattern));
}

#[test]
fn brace_alternation_comma_is_not_a_split() {
    // If brace-awareness is broken, this would split into `{src/**` and
    // `tests/**}` — both are invalid globs and would match nothing. A
    // correctly preserved brace expression matches `src/` OR `tests/`.
    let pattern = "{src/database/*.rs,tests/**/*.rs}";
    assert!(matches_glob_pattern("src/database/workspace.rs", pattern));
    assert!(matches_glob_pattern("tests/integration/foo.rs", pattern));
}

// ---------------------------------------------------------------------------
// Exclusions
// ---------------------------------------------------------------------------

#[test]
fn mixed_include_and_exclude() {
    let pattern = "!docs/**,src/**";
    assert!(
        matches_glob_pattern("src/lib.rs", pattern),
        "src/lib.rs is included by src/** and not excluded",
    );
    assert!(
        !matches_glob_pattern("docs/README.md", pattern),
        "docs/README.md is excluded by !docs/**",
    );
}

#[test]
fn exclusion_only_implies_include_all() {
    // Exclusion-only: everything matches EXCEPT the excluded set.
    let pattern = "!docs/**";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(matches_glob_pattern("tests/integration/foo.rs", pattern));
    assert!(!matches_glob_pattern("docs/README.md", pattern));
    assert!(!matches_glob_pattern("docs/nested/page.md", pattern));
}

#[test]
fn multiple_exclusions_combine() {
    let pattern = "!docs/**,!target/**";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(!matches_glob_pattern("docs/README.md", pattern));
    assert!(!matches_glob_pattern("target/debug/foo", pattern));
}

#[test]
fn inclusion_and_exclusion_order_independent() {
    // Same pattern, swapped order: result must be identical.
    let a = "!docs/**,src/**";
    let b = "src/**,!docs/**";
    for path in ["src/lib.rs", "docs/README.md", "tests/foo.rs"] {
        assert_eq!(
            matches_glob_pattern(path, a),
            matches_glob_pattern(path, b),
            "order of segments must not affect outcome for {}",
            path,
        );
    }
}

// ---------------------------------------------------------------------------
// Whitespace handling — pinned regressions for literal-space globs
// ---------------------------------------------------------------------------

#[test]
fn literal_space_in_glob_is_preserved() {
    // This mirrors the pinned regression test at
    // src/tests/integration/search_regression_tests.rs:253-260.
    // A space inside a glob component must NOT be treated as a split separator.
    let path = "\\\\?\\C:\\source\\My Project\\src\\file name.rs";
    let pattern = "**/file name.rs";
    assert!(
        matches_glob_pattern(path, pattern),
        "literal space in glob should match path with literal space",
    );
}

#[test]
fn whitespace_between_globs_is_not_a_split() {
    // Whitespace is NOT a separator — "a/** b/**" is a single literal pattern
    // (globset likely rejects or never matches it). Must not split into
    // two patterns.
    let pattern = "a/** b/**";
    assert!(!matches_glob_pattern("a/foo.rs", pattern));
    assert!(!matches_glob_pattern("b/foo.rs", pattern));
    assert!(!matches_glob_pattern("src/a/b/foo.rs", pattern));
}

// ---------------------------------------------------------------------------
// Single-pattern legacy behavior preserved
// ---------------------------------------------------------------------------

#[test]
fn single_simple_filename_still_basename_matches() {
    // Existing behavior from matches_glob_pattern: simple filename (no
    // wildcards, no separators) matches against basename, tolerating UNC
    // paths.
    let path = "\\\\?\\C:\\source\\proj\\Program.cs";
    assert!(matches_glob_pattern(path, "Program.cs"));
}

#[test]
fn single_exclusion_still_works() {
    let path = "docs/README.md";
    assert!(!matches_glob_pattern(path, "!docs/**"));
    assert!(matches_glob_pattern("src/lib.rs", "!docs/**"));
}

#[test]
fn trailing_empty_segment_after_comma_is_ignored() {
    // "src/**," trailing empty after comma must not mean "include all"
    // (which would flip to implicit include-all since there'd be no
    // effective inclusion). The non-empty segment stays the only inclusion.
    let pattern = "src/**,";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(!matches_glob_pattern("docs/README.md", pattern));
}

#[test]
fn whitespace_around_comma_segments_is_trimmed() {
    let pattern = " src/** , tests/** ";
    assert!(matches_glob_pattern("src/lib.rs", pattern));
    assert!(matches_glob_pattern("tests/foo.rs", pattern));
    assert!(!matches_glob_pattern("docs/README.md", pattern));
}
