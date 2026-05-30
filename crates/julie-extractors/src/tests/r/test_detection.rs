//! R testthat call-style test detection (Miller bridge test-roles).
//!
//! testthat declares tests as call expressions — classic `test_that("desc", {})`
//! and BDD-style `describe("desc", { it("desc", {}) })` — not named functions.
//! The R extractor recognizes these via the shared `crate::test_calls` core and
//! emits the canonical `is_test` / `test_container` metadata, byte-identical to
//! the JS/TS, Dart, and Lua call-style paths. These tests assert that metadata on
//! the public `extract_symbols` output and confirm that non-DSL calls
//! (`expect_equal`, `library`) do NOT become test symbols.

use super::extract_symbols;
use crate::base::Symbol;

fn meta_bool(s: &Symbol, key: &str) -> bool {
    s.metadata
        .as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn r_qualified_callee_is_not_materialized() {
    // FALSE-POSITIVE GUARD (#66): `classify_call` keys on the segment before the
    // first '.'. In R, '.' is a normal identifier char (S3 dispatch names like
    // `print.data.frame`), NOT a member operator — so an ordinary S3-style call
    // `describe.default("x")` is a single dotted identifier that would otherwise
    // be misclassified as a testthat `describe`. Only an exact bare vocab name is
    // a DSL call; any dotted callee must be rejected.
    let code = r#"
describe.default("widget config")
"#;
    let syms = extract_symbols(code);
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "dotted S3-style callee `describe.default(...)` must not materialize a test symbol, got {syms:?}"
    );
}

#[test]
fn testthat_test_that_describe_it_emit_test_role_metadata() {
    let code = r#"
test_that("addition works", {
  expect_equal(1 + 1, 2)
})

describe("a widget", {
  it("renders correctly", {
    expect_true(TRUE)
  })
})
"#;
    let syms = extract_symbols(code);

    let tt = syms
        .iter()
        .find(|s| s.name == "addition works")
        .unwrap_or_else(|| panic!("expected a test_that symbol, got {syms:?}"));
    assert!(meta_bool(tt, "is_test"), "test_that() is a test case");

    let desc = syms
        .iter()
        .find(|s| s.name == "a widget")
        .unwrap_or_else(|| panic!("expected a describe container symbol, got {syms:?}"));
    assert!(
        meta_bool(desc, "test_container"),
        "describe() is a test container"
    );

    let it = syms
        .iter()
        .find(|s| s.name == "renders correctly")
        .unwrap_or_else(|| panic!("expected an `it` symbol, got {syms:?}"));
    assert!(meta_bool(it, "is_test"), "it() is a test case");
}

#[test]
fn non_dsl_r_calls_do_not_become_test_symbols() {
    // `expect_equal(...)` and `library(...)` are not testthat DSL containers/cases.
    let code = r#"
expect_equal(1 + 1, 2)
library(testthat)
"#;
    let syms = extract_symbols(code);
    assert_eq!(
        syms.iter()
            .filter(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container"))
            .count(),
        0,
        "expect_equal / library must not carry test-role metadata: {syms:?}"
    );
}
