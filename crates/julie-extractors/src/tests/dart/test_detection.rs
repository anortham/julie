//! Dart test-role detection signals (Miller bridge test-roles).
//!
//! EXTRACTOR-level assertions; the role classifier lives in the `julie` crate.
//! Dart's primary style is call-based (`package:test`): `test()`/`group()`/
//! `setUp()` are call expressions, not named declarations, so the extractor
//! materializes each as a `Function` symbol carrying `is_test` /
//! `test_container` / `test_lifecycle` metadata (mirroring the JS/TS path). The
//! secondary style is annotations (`@isTest` / `@test`) on real functions,
//! already wired through `is_test_symbol` + annotation markers.

use crate::base::SymbolKind;
use crate::dart::DartExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn symbols(code: &str, file: &str) -> Vec<crate::base::Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_dart::LANGUAGE.into())
        .expect("load Dart grammar");
    let tree = parser.parse(code, None).expect("parse Dart");
    let mut ext = DartExtractor::new(
        "dart".to_string(),
        file.to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    ext.extract_symbols(&tree)
}

fn meta_bool(symbol: &crate::base::Symbol, key: &str) -> bool {
    symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn annotation_keys(symbol: &crate::base::Symbol) -> Vec<String> {
    symbol
        .annotations
        .iter()
        .map(|a| a.annotation_key.clone())
        .collect()
}

#[test]
fn dart_qualified_callee_is_not_materialized() {
    // FALSE-POSITIVE GUARD (#66): `classify_call` keys on the segment before the
    // first '.', so a member call whose RECEIVER is a vocab word
    // (`test.configure('x', ...)`) would otherwise be misclassified as a
    // package:test `test`. Only a bare-identifier callee is a DSL call.
    let code = r#"
void run() {
  test.configure('feature', () {});
}
"#;
    let syms = symbols(code, "lib/run.dart");
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "qualified callee `test.configure(...)` must not materialize a test symbol, got {syms:?}"
    );
}

#[test]
fn test_call_materialized_as_is_test_symbol() {
    // `test('adds', () {})` — a package:test call. The extractor must materialize
    // a Function symbol named "adds" (the description) flagged is_test. Call-style
    // is definitive, so no test path is required (matches the JS/TS path).
    let code = r#"
import 'package:test/test.dart';

void main() {
  test('adds two numbers', () {
    expect(2 + 2, equals(4));
  });
}
"#;
    let syms = symbols(code, "lib/calc.dart");
    let test_sym = syms
        .iter()
        .find(|s| s.name == "adds two numbers")
        .unwrap_or_else(|| panic!("expected materialized test symbol, got {syms:?}"));
    assert_eq!(test_sym.kind, SymbolKind::Function);
    assert!(
        meta_bool(test_sym, "is_test"),
        "test() call symbol must be flagged is_test"
    );
}

#[test]
fn group_is_container_with_nested_test_parented() {
    // `group('math', () { test('x', () {}) })` — the group becomes a
    // test_container symbol and the nested test parents to it (mirroring JS
    // `describe`). The inner test is still is_test.
    let code = r#"
import 'package:test/test.dart';

void main() {
  group('math', () {
    test('adds', () {
      expect(2 + 2, 4);
    });
  });
}
"#;
    let syms = symbols(code, "test/math_test.dart");
    let group_sym = syms
        .iter()
        .find(|s| s.name == "math")
        .unwrap_or_else(|| panic!("expected group symbol, got {syms:?}"));
    assert!(
        meta_bool(group_sym, "test_container"),
        "group() call must be flagged test_container"
    );
    let test_sym = syms
        .iter()
        .find(|s| s.name == "adds")
        .unwrap_or_else(|| panic!("expected nested test symbol, got {syms:?}"));
    assert!(
        meta_bool(test_sym, "is_test"),
        "nested test must be is_test"
    );
    assert_eq!(
        test_sym.parent_id.as_deref(),
        Some(group_sym.id.as_str()),
        "nested test must parent to the enclosing group"
    );
}

#[test]
fn setup_lifecycle_call_materialized() {
    // `setUp(() {})` — a lifecycle fixture. Materialized with the callee name and
    // both is_test + test_lifecycle metadata.
    let code = r#"
import 'package:test/test.dart';

void main() {
  setUp(() {
    print('before each');
  });
}
"#;
    let syms = symbols(code, "test/widget_test.dart");
    let setup = syms
        .iter()
        .find(|s| s.name == "setUp")
        .unwrap_or_else(|| panic!("expected setUp symbol, got {syms:?}"));
    assert!(meta_bool(setup, "is_test"), "setUp must be is_test");
    assert!(
        meta_bool(setup, "test_lifecycle"),
        "setUp must be flagged test_lifecycle"
    );
}

#[test]
fn non_test_calls_are_not_materialized() {
    // `print(...)` / `expect(...)` are ordinary calls, not test runners. They must
    // NOT become symbols — only the recognized test DSL calls do.
    let code = r#"
import 'package:test/test.dart';

void main() {
  print('hello');
  expect(1, 1);
}
"#;
    let syms = symbols(code, "test/noise_test.dart");
    assert!(
        !syms.iter().any(|s| s.name == "hello" || s.name == "print"),
        "print() must not be materialized as a symbol, got {syms:?}"
    );
    assert!(
        !syms.iter().any(|s| meta_bool(s, "test_container")),
        "no container should be produced for non-test calls, got {syms:?}"
    );
}

#[test]
fn istest_annotation_on_function_flags_is_test() {
    // `@isTest void verify(...)` (package:meta) — an annotated test helper. The
    // function carries annotation_key "istest" and the is_test flag, independent
    // of the call-style path.
    let code = r#"
import 'package:meta/meta.dart';

@isTest
void verifyBehavior() {
  expect(true, isTrue);
}
"#;
    let syms = symbols(code, "test/helpers.dart");
    let func = syms
        .iter()
        .find(|s| s.name == "verifyBehavior")
        .unwrap_or_else(|| panic!("expected verifyBehavior function, got {syms:?}"));
    assert!(
        annotation_keys(func).iter().any(|k| k == "istest"),
        "@isTest must yield annotation_key 'istest', got {:?}",
        annotation_keys(func)
    );
    assert!(
        meta_bool(func, "is_test"),
        "@isTest function must be flagged is_test"
    );
}
