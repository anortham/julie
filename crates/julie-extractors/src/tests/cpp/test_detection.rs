//! C++ test detection (Miller bridge test-role work).
//!
//! C++ has no annotation/attribute test markers, so detection is structural +
//! base-type driven, verified here against real extractor output:
//!
//!   - GoogleTest `TEST` / `TEST_F` / `TEST_P` / `TYPED_TEST` / `TYPED_TEST_P`
//!     parse as `function_definition`s whose declarator identifier is the macro
//!     keyword and whose two "parameters" are the suite/fixture and the test name.
//!     The extractor (`cpp/functions.rs`) rebuilds a `Suite.Name` symbol, sets
//!     `is_test=true` STRUCTURALLY (a graceful fallback), AND attaches a SYNTHETIC
//!     annotation whose key is the lowercased macro keyword (`test`, `test_f`,
//!     `test_p`, `typed_test`, `typed_test_p`). The post-extraction role classifier
//!     (a main-crate concern) maps that key via `languages/cpp.toml`
//!     `[annotation_classes.test]` to a ROLE — `_P` variants →
//!     `parameterized_test`, the rest → `test_case` — which is the whole reason for
//!     the annotation (structural is_test alone would collapse the `_P` distinction).
//!     There is no GoogleTest arm in `test_detection.rs`. These extractor tests lock
//!     the rename + is_test + per-macro annotation key (the inputs the classifier
//!     consumes); the annotation-key → role mapping is asserted in the main crate
//!     where the classifier runs.
//!
//!   - GoogleTest fixtures subclass `::testing::Test` / `::testing::TestWithParam<T>`.
//!     The extractor records clean `base_types` (access specifier dropped, template
//!     args stripped) so `src/analysis/test_roles.rs` can flag the fixture class as
//!     a `TestContainer` via the `test_base_types` config in `languages/cpp.toml`.
//!     (That `TestContainer` classification is a main-crate concern and is covered
//!     there; here we lock the extractor's `base_types` output that feeds it.)
//!
//!   - Catch2 `TEST_CASE("...")` parses as a `call_expression` statement with a
//!     DETACHED block — no `function_definition` symbol is emitted, so it is NOT
//!     handled here. It is call-style and belongs to `test_calls.rs`.

use crate::base::{Symbol, SymbolKind};
use crate::cpp::CppExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

/// Extract symbols from C++ source at a NON-test path (`src/`), proving the
/// GoogleTest detection is structural rather than name/path-based.
fn symbols(code: &str) -> Vec<Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cpp::LANGUAGE.into())
        .expect("load C++ grammar");
    let tree = parser.parse(code, None).expect("parse C++");
    let mut ext = CppExtractor::new(
        "src/math.cpp".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    ext.extract_symbols(&tree)
}

fn is_test(sym: &Symbol) -> bool {
    sym.metadata
        .as_ref()
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// True if the symbol carries an annotation marker with the given (lowercased)
/// key — the signal the role classifier consumes for GoogleTest macros.
fn has_annotation_key(sym: &Symbol, key: &str) -> bool {
    sym.annotations.iter().any(|a| a.annotation_key == key)
}

fn base_types(sym: &Symbol) -> Vec<String> {
    sym.metadata
        .as_ref()
        .and_then(|m| m.get("base_types"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Last `.`/`:`-delimited segment — mirrors `test_roles::last_type_segment`, the
/// matching the role classifier uses against `test_base_types`.
fn last_segment(name: &str) -> &str {
    name.rsplit(['.', ':']).next().unwrap_or(name).trim()
}

#[test]
fn googletest_test_macro_is_named_suite_dot_name_with_test_annotation() {
    let syms = symbols(
        r#"
TEST(MathTest, AdditionWorks) {
    EXPECT_EQ(4, 2 + 2);
}
"#,
    );
    let t = syms
        .iter()
        .find(|s| s.name == "MathTest.AdditionWorks")
        .unwrap_or_else(|| panic!("expected a `Suite.Name` symbol for TEST, got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
    assert!(
        is_test(t),
        "GoogleTest TEST must be is_test=true structurally (path-independent), got {t:?}"
    );
    assert!(
        has_annotation_key(t, "test"),
        "GoogleTest TEST must carry the synthetic `test` annotation key, got {:?}",
        t.annotations
    );
}

#[test]
fn googletest_test_f_and_typed_test_carry_test_case_annotation_keys() {
    // TEST_F (fixture) and TYPED_TEST share the macro shape; both rename to
    // `Suite.Name` and carry the plain (non-parameterized) annotation key that
    // cpp.toml maps to `test_case`.
    let syms = symbols(
        r#"
TEST_F(MathFixture, AdditionWorks) {
    EXPECT_EQ(4, 2 + 2);
}
TYPED_TEST(MathTyped, AdditionWorks) {
    EXPECT_EQ(4, 2 + 2);
}
"#,
    );
    for (expected, key) in [
        ("MathFixture.AdditionWorks", "test_f"),
        ("MathTyped.AdditionWorks", "typed_test"),
    ] {
        let t = syms
            .iter()
            .find(|s| s.name == expected)
            .unwrap_or_else(|| panic!("expected `{expected}`, got {syms:?}"));
        assert_eq!(t.kind, SymbolKind::Function);
        assert!(is_test(t), "{expected} must be is_test=true, got {t:?}");
        assert!(
            has_annotation_key(t, key),
            "{expected} must carry the `{key}` annotation key, got {:?}",
            t.annotations
        );
    }
}

#[test]
fn googletest_parameterized_macros_carry_p_variant_annotation_keys() {
    // The `_P` variants are the whole reason for the annotation approach: they map
    // to `parameterized_test`, not `test_case`. Lock the distinct annotation keys.
    let syms = symbols(
        r#"
TEST_P(MathParam, Squares) {
    EXPECT_GT(GetParam() * GetParam(), 0);
}
TYPED_TEST_P(MathTypedParam, Squares) {
    EXPECT_GT(1, 0);
}
"#,
    );
    for (expected, key) in [
        ("MathParam.Squares", "test_p"),
        ("MathTypedParam.Squares", "typed_test_p"),
    ] {
        let t = syms
            .iter()
            .find(|s| s.name == expected)
            .unwrap_or_else(|| panic!("expected `{expected}`, got {syms:?}"));
        assert!(is_test(t), "{expected} must be is_test=true, got {t:?}");
        assert!(
            has_annotation_key(t, key),
            "{expected} must carry the `{key}` annotation key (→ parameterized_test), got {:?}",
            t.annotations
        );
    }
}

#[test]
fn non_macro_two_arg_function_is_not_renamed_or_annotated() {
    // Guard against over-eager macro detection: an ordinary 2-arg function whose
    // name is NOT a GoogleTest macro must keep its own name and carry no synthetic
    // test annotation.
    let syms = symbols(
        r#"
int add(int a, int b) {
    return a + b;
}
"#,
    );
    let f = syms
        .iter()
        .find(|s| s.name == "add")
        .unwrap_or_else(|| panic!("expected the `add` function, got {syms:?}"));
    assert!(
        !is_test(f),
        "an ordinary function must not be is_test, got {f:?}"
    );
    let test_keys = ["test", "test_f", "test_p", "typed_test", "typed_test_p"];
    assert!(
        !test_keys.iter().any(|k| has_annotation_key(f, k)),
        "an ordinary function must not carry a GoogleTest annotation, got {:?}",
        f.annotations
    );
}

#[test]
fn googletest_fixture_class_records_base_types_for_container_match() {
    // `class X : public ::testing::Test` — the extractor must record a clean
    // `base_types` whose last segment is `Test` so the role classifier flags the
    // fixture as a TestContainer via `test_base_types = ["testing::Test"]`.
    let syms = symbols(
        r#"
class DatabaseTest : public ::testing::Test {
protected:
    void SetUp() override {}
};
"#,
    );
    let cls = syms
        .iter()
        .find(|s| s.name == "DatabaseTest")
        .unwrap_or_else(|| panic!("expected the fixture class symbol, got {syms:?}"));
    assert_eq!(cls.kind, SymbolKind::Class);
    let bases = base_types(cls);
    assert!(
        bases.iter().any(|b| last_segment(b) == "Test"),
        "fixture base_types must contain a `::testing::Test` base, got {bases:?}"
    );
}

#[test]
fn googletest_parameterized_fixture_strips_template_args_from_base_types() {
    // `class X : public ::testing::TestWithParam<int>` — the recorded base type
    // must be the generic head `::testing::TestWithParam` (no `<int>`), otherwise
    // last-segment matching against `test_base_types = ["testing::TestWithParam"]`
    // would fail on `TestWithParam<int>`.
    let syms = symbols(
        r#"
class ParameterizedMathTest : public ::testing::TestWithParam<int> {
};
"#,
    );
    let cls = syms
        .iter()
        .find(|s| s.name == "ParameterizedMathTest")
        .unwrap_or_else(|| panic!("expected the parameterized fixture class, got {syms:?}"));
    let bases = base_types(cls);
    assert!(
        bases.iter().all(|b| !b.contains('<')),
        "template args must be stripped from base_types, got {bases:?}"
    );
    assert!(
        bases.iter().any(|b| last_segment(b) == "TestWithParam"),
        "parameterized fixture base_types must contain `::testing::TestWithParam`, got {bases:?}"
    );
}
