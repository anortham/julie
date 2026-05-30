//! C++ Catch2 call-style test extraction (Miller bridge test-roles).
//!
//! Catch2's `TEST_CASE("name", "[tag]") { ... }` (and `SECTION`, `SCENARIO`,
//! `TEST_CASE_METHOD`) parse as `call_expression`s, not function_definitions, so
//! they are materialized via the shared `crate::test_calls` core through
//! `cpp/test_calls.rs`. The display name is the first `string_literal` argument.

use super::extract_symbols;
use crate::base::{Symbol, SymbolKind};

fn is_test(sym: &Symbol) -> bool {
    sym.metadata
        .as_ref()
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn is_container(sym: &Symbol) -> bool {
    sym.metadata
        .as_ref()
        .and_then(|m| m.get("test_container"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn catch2_test_case_is_named_from_first_string_and_flagged() {
    let syms = extract_symbols(
        r#"
TEST_CASE("vector grows", "[vector]") {
    REQUIRE(1 == 1);
}
"#,
    );
    let t = syms
        .iter()
        .find(|s| s.name == "vector grows")
        .unwrap_or_else(|| panic!("expected a TEST_CASE symbol `vector grows`, got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
    assert!(is_test(t), "Catch2 TEST_CASE must be is_test=true, got {t:?}");
    assert!(
        !is_container(t),
        "a TEST_CASE is a test, not a container, got {t:?}"
    );
}

#[test]
fn catch2_section_is_a_container_not_a_test() {
    let syms = extract_symbols(
        r#"
TEST_CASE("ops") {
    SECTION("push_back grows") {
        REQUIRE(1);
    }
}
"#,
    );
    let section = syms
        .iter()
        .find(|s| s.name == "push_back grows")
        .unwrap_or_else(|| panic!("expected a SECTION symbol, got {syms:?}"));
    assert!(
        is_container(section),
        "SECTION must be test_container=true, got {section:?}"
    );
    assert!(
        !is_test(section),
        "SECTION is a container, not a test case, got {section:?}"
    );
    // The enclosing TEST_CASE is still captured as a test.
    assert!(
        syms.iter().any(|s| s.name == "ops" && is_test(s)),
        "the enclosing TEST_CASE `ops` must be is_test, got {syms:?}"
    );
}

#[test]
fn catch2_scenario_is_a_test() {
    let syms = extract_symbols(
        r#"
SCENARIO("user logs in", "[auth]") {
    REQUIRE(1);
}
"#,
    );
    let s = syms
        .iter()
        .find(|s| s.name == "user logs in")
        .unwrap_or_else(|| panic!("expected a SCENARIO symbol, got {syms:?}"));
    assert!(is_test(s), "Catch2 SCENARIO must be is_test=true, got {s:?}");
}

#[test]
fn catch2_test_case_method_takes_name_from_the_string_arg() {
    // `TEST_CASE_METHOD(Fixture, "name", "[tag]")` — the first arg is the fixture
    // identifier; the name is the SECOND argument (the first string literal).
    let syms = extract_symbols(
        r#"
TEST_CASE_METHOD(DatabaseFixture, "query runs", "[db]") {
    REQUIRE(1);
}
"#,
    );
    let t = syms
        .iter()
        .find(|s| s.name == "query runs")
        .unwrap_or_else(|| panic!("expected a TEST_CASE_METHOD symbol `query runs`, got {syms:?}"));
    assert!(is_test(t), "TEST_CASE_METHOD must be is_test=true, got {t:?}");
}

#[test]
fn non_catch2_call_is_not_materialized() {
    // Plain assertion/calls must NOT become test symbols.
    let syms = extract_symbols(
        r#"
void run() {
    REQUIRE(1 == 1);
    compute(2, 3);
}
"#,
    );
    assert!(
        !syms.iter().any(|s| is_test(s) || is_container(s)),
        "no plain call should be flagged is_test/test_container, got {syms:?}"
    );
}
