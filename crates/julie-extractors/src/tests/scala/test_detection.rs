//! Scala ScalaTest / MUnit call-style test detection (Miller bridge, Wave-3).
//!
//! ScalaTest and MUnit express tests as CALL expressions, not named methods:
//!   - FunSuite / MUnit: `test("name") { ... }`
//!   - FunSpec:          `describe("subject") { it("behaves") { ... } }`
//!   - FlatSpec:         `"subject" should "behave" in { ... }`  (infix form)
//! The declaration-walking extractor only flags `def` methods via a path
//! heuristic, so these are invisible today. The adapter (`scala/test_calls.rs`)
//! walks the grammar locally and delegates to the shared `crate::test_calls`
//! core for classification + symbol construction.

use crate::base::SymbolKind;
use crate::scala::ScalaExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn symbols(code: &str, file: &str) -> Vec<crate::base::Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_scala::LANGUAGE.into())
        .expect("load Scala grammar");
    let tree = parser.parse(code, None).expect("parse Scala");
    let mut ext = ScalaExtractor::new(
        "scala".to_string(),
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

#[test]
fn scala_qualified_callee_is_not_materialized() {
    // FALSE-POSITIVE GUARD (#66): `classify_call` keys on the segment before the
    // first '.', so a curried member call whose RECEIVER is a vocab word
    // (`feature.enable("flag") { }`, inner callee = `field_expression`
    // "feature.enable") would otherwise be misclassified as a `feature` container.
    // Only a bare-identifier inner callee is a DSL clause.
    let code = r#"object Demo {
  def run(): Unit = {
    feature.enable("flag") {
      register()
    }
  }
}
"#;
    let syms = symbols(code, "src/main/scala/Demo.scala");
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "qualified callee `feature.enable(...) {{ }}` must not materialize a test symbol, got {syms:?}"
    );
}

#[test]
fn funsuite_test_call_materialized_is_test() {
    // FunSuite/MUnit `test("name") { ... }` → Function symbol "name" flagged is_test.
    let code = r#"class CalcSuite extends AnyFunSuite {
  test("adds two numbers") {
    assert(1 + 1 == 2)
  }
}
"#;
    let syms = symbols(code, "src/test/scala/CalcSuite.scala");
    let t = syms
        .iter()
        .find(|s| s.name == "adds two numbers")
        .unwrap_or_else(|| panic!("expected materialized test symbol, got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
    assert!(meta_bool(t, "is_test"), "test(...) call must be is_test");
}

#[test]
fn funspec_describe_container_with_nested_it() {
    // FunSpec `describe("subject") { it("behaves") { ... } }` → describe is a
    // test_container, the nested it is is_test and parents to the describe.
    let code = r#"class SpecSuite extends AnyFunSpec {
  describe("math") {
    it("adds numbers") {
      assert(true)
    }
  }
}
"#;
    let syms = symbols(code, "src/test/scala/SpecSuite.scala");
    let desc = syms
        .iter()
        .find(|s| s.name == "math" && meta_bool(s, "test_container"))
        .unwrap_or_else(|| panic!("expected describe container, got {syms:?}"));
    let it = syms
        .iter()
        .find(|s| s.name == "adds numbers")
        .unwrap_or_else(|| panic!("expected nested it symbol, got {syms:?}"));
    assert!(meta_bool(it, "is_test"), "it(...) must be is_test");
    assert_eq!(
        it.parent_id.as_deref(),
        Some(desc.id.as_str()),
        "nested it must parent to the describe container"
    );
}

#[test]
fn flatspec_infix_clause_materialized_is_test() {
    // FlatSpec `"subject" should "behaviour" in { ... }` (infix form) → a Function
    // symbol named "subject should behaviour" flagged is_test. This is the
    // stretch infix form; the grammar is `infix_expression(in){ left=infix(should),
    // right=block }`.
    let code = r#"class StackSpec extends AnyFlatSpec {
  "A Stack" should "pop values in LIFO order" in {
    assert(true)
  }
}
"#;
    let syms = symbols(code, "src/test/scala/StackSpec.scala");
    let t = syms
        .iter()
        .find(|s| s.name == "A Stack should pop values in LIFO order")
        .unwrap_or_else(|| panic!("expected FlatSpec test symbol, got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
    assert!(meta_bool(t, "is_test"), "FlatSpec clause must be is_test");
}

#[test]
fn non_test_calls_are_not_materialized() {
    // Negative control. Exercises every guard in the adapter:
    //  - `render("widget") { ... }` — curried-block call but callee NOT in vocab.
    //  - `println("hello")` — ordinary call (no block body).
    //  - `1 + 2` — arithmetic infix (operator is not `in`).
    //  - `"a" plus "b" in { ... }` — `in` infix but the left verb is not a
    //    behaviour verb (should/must/can/will).
    // NONE may produce a test symbol or test-role metadata.
    let code = r#"object Demo {
  def run(): Unit = {
    render("widget") {
      draw()
    }
    println("hello")
    val x = 1 + 2
    "a" plus "b" in {
      ignored()
    }
  }
}
"#;
    let syms = symbols(code, "src/main/scala/Demo.scala");
    assert!(
        !syms
            .iter()
            .any(|s| s.name == "widget" || s.name == "hello" || s.name == "a plus b"),
        "non-test calls must not materialize test symbols, got {syms:?}"
    );
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "no test-role metadata should appear for non-test code, got {syms:?}"
    );
}
