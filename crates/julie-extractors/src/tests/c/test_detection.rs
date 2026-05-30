//! C test detection (Miller bridge test-role work).
//!
//! C's symbol-emitting test frameworks declare tests as ordinary functions named
//! `test_*`: Unity (`void test_Foo(void)`) and CMocka (`static void
//! test_bar(void **state)`). Both are caught by the name+path generic detector in
//! test_detection.rs (`test_` prefix in a test path), which is why these are
//! regression-locked here rather than needing a bespoke `detect_c` arm.
//!
//! Criterion (`Test(suite, name) { ... }`) is intentionally NOT covered here: the
//! grammar parses it as a `call_expression` statement with a DETACHED block, so no
//! `function_definition` symbol is emitted. That is call-style and is handled by
//! `test_calls.rs`, not by symbol-level detection.

use super::extract_symbols_with_name;
use crate::base::Symbol;

fn is_test(sym: &Symbol) -> bool {
    sym.metadata
        .as_ref()
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn unity_test_function_is_flagged() {
    // Unity: `void test_*(void)` in a test file.
    let syms = extract_symbols_with_name(
        r#"
void test_AdditionWorks(void) {
    TEST_ASSERT_EQUAL(4, 2 + 2);
}
"#,
        "tests/test_math.c",
    );
    let t = syms
        .iter()
        .find(|s| s.name == "test_AdditionWorks")
        .unwrap_or_else(|| panic!("expected the Unity test function, got {syms:?}"));
    assert!(is_test(t), "Unity test_* function must be is_test=true");
}

#[test]
fn cmocka_test_function_is_flagged() {
    // CMocka: `static void test_*(void **state)` in a test file.
    let syms = extract_symbols_with_name(
        r#"
static void test_null_pointer(void **state) {
    assert_null(NULL);
}
"#,
        "tests/test_ptr.c",
    );
    let t = syms
        .iter()
        .find(|s| s.name == "test_null_pointer")
        .unwrap_or_else(|| panic!("expected the CMocka test function, got {syms:?}"));
    assert!(is_test(t), "CMocka test_* function must be is_test=true");
}

#[test]
fn non_test_function_in_test_file_is_not_flagged() {
    // Guard against over-flagging: a helper without the `test_` prefix in the same
    // test file must NOT be marked a test.
    let syms = extract_symbols_with_name(
        r#"
void setup_fixture(void) {
    init_db();
}
"#,
        "tests/test_math.c",
    );
    let h = syms
        .iter()
        .find(|s| s.name == "setup_fixture")
        .unwrap_or_else(|| panic!("expected the helper function, got {syms:?}"));
    assert!(
        !is_test(h),
        "a non-test_* helper must not be flagged is_test, got {h:?}"
    );
}
