//! Kotlin Kotest / Spek call-style test detection (Miller bridge, Wave-3).
//!
//! Kotest and Spek express tests as call expressions, not named function
//! declarations or class annotations:
//!   - DescribeSpec: `describe("subject") { it("behaves") { … } }`
//!   - FunSpec:      `test("name") { … }`, `context("group") { … }`
//!   - BehaviorSpec: `given("…") { When("…") { then("…") { } } }`
//!   - ShouldSpec:   `should("name") { … }`
//!   - Spek:         `describe("…") { it("…") { } }`, `beforeEachTest { }`
//!
//! The dominant Kotlin test idiom (JUnit annotations) is already handled by the
//! declaration/annotation path. This adapter is additive — it materializes the
//! call-DSL forms that were previously invisible to the extractor.

use crate::base::SymbolKind;
use crate::kotlin::KotlinExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn symbols(code: &str, file: &str) -> Vec<crate::base::Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
        .expect("load Kotlin grammar");
    let tree = parser.parse(code, None).expect("parse Kotlin");
    let mut ext = KotlinExtractor::new(
        "kotlin".to_string(),
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

// ---------------------------------------------------------------------------
// Kotest DescribeSpec
// ---------------------------------------------------------------------------

#[test]
fn kotest_describespec_it_is_test() {
    // DescribeSpec `it("name") { }` → Function symbol named "name" flagged is_test.
    let code = r#"class CalcSpec : DescribeSpec({
  describe("calculator") {
    it("adds two numbers") {
      1 + 1 shouldBe 2
    }
  }
})
"#;
    let syms = symbols(code, "src/test/kotlin/CalcSpec.kt");
    let t = syms
        .iter()
        .find(|s| s.name == "adds two numbers")
        .unwrap_or_else(|| panic!("expected materialized it() test symbol; got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
    assert!(meta_bool(t, "is_test"), "it(...) must set is_test");
    assert!(!meta_bool(t, "test_container"), "it() must not be a container");
}

#[test]
fn kotest_describespec_describe_is_container() {
    // DescribeSpec `describe("subject") { }` → Function symbol flagged test_container.
    let code = r#"class CalcSpec : DescribeSpec({
  describe("calculator") {
    it("adds") { }
  }
})
"#;
    let syms = symbols(code, "src/test/kotlin/CalcSpec.kt");
    let d = syms
        .iter()
        .find(|s| s.name == "calculator" && meta_bool(s, "test_container"))
        .unwrap_or_else(|| panic!("expected describe() container; got {syms:?}"));
    assert_eq!(d.kind, SymbolKind::Function);
    assert!(!meta_bool(d, "is_test"), "describe() must not set is_test");
}

#[test]
fn kotest_describespec_nested_it_parents_to_describe() {
    // Nested `it` must record the enclosing `describe` as its parent.
    let code = r#"class ParentSpec : DescribeSpec({
  describe("math") {
    it("adds numbers") {
      assert(true)
    }
  }
})
"#;
    let syms = symbols(code, "src/test/kotlin/ParentSpec.kt");
    let desc = syms
        .iter()
        .find(|s| s.name == "math" && meta_bool(s, "test_container"))
        .unwrap_or_else(|| panic!("expected describe container; got {syms:?}"));
    let it = syms
        .iter()
        .find(|s| s.name == "adds numbers" && meta_bool(s, "is_test"))
        .unwrap_or_else(|| panic!("expected it() test symbol; got {syms:?}"));
    assert_eq!(
        it.parent_id.as_deref(),
        Some(desc.id.as_str()),
        "it() must parent to the enclosing describe()"
    );
}

// ---------------------------------------------------------------------------
// Kotest FunSpec
// ---------------------------------------------------------------------------

#[test]
fn kotest_funspec_test_is_test() {
    // FunSpec `test("name") { }` → Function symbol flagged is_test.
    let code = r#"class MathSpec : FunSpec({
  test("addition returns correct result") {
    1 + 1 shouldBe 2
  }
})
"#;
    let syms = symbols(code, "src/test/kotlin/MathSpec.kt");
    let t = syms
        .iter()
        .find(|s| s.name == "addition returns correct result")
        .unwrap_or_else(|| panic!("expected test() symbol; got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
    assert!(meta_bool(t, "is_test"), "test(...) must set is_test");
}

#[test]
fn kotest_funspec_context_is_container() {
    // FunSpec `context("group") { test("…") { } }` → context is test_container.
    let code = r#"class MathSpec : FunSpec({
  context("arithmetic") {
    test("adds") { }
    test("subtracts") { }
  }
})
"#;
    let syms = symbols(code, "src/test/kotlin/MathSpec.kt");
    let ctx = syms
        .iter()
        .find(|s| s.name == "arithmetic" && meta_bool(s, "test_container"))
        .unwrap_or_else(|| panic!("expected context() container; got {syms:?}"));
    assert_eq!(ctx.kind, SymbolKind::Function);
}

// ---------------------------------------------------------------------------
// Kotest BehaviorSpec
// ---------------------------------------------------------------------------

#[test]
fn kotest_behaviorspec_given_when_then() {
    // BehaviorSpec `given { When { then { } } }`:
    //   given → container, When → container, then → test.
    let code = r#"class BehaviorTest : BehaviorSpec({
  given("a calculator") {
    When("adding two numbers") {
      then("should return the correct sum") {
        1 + 1 shouldBe 2
      }
    }
  }
})
"#;
    let syms = symbols(code, "src/test/kotlin/BehaviorTest.kt");
    let given = syms
        .iter()
        .find(|s| s.name == "a calculator" && meta_bool(s, "test_container"))
        .unwrap_or_else(|| panic!("expected given() container; got {syms:?}"));
    assert_eq!(given.kind, SymbolKind::Function);

    let when_sym = syms
        .iter()
        .find(|s| s.name == "adding two numbers" && meta_bool(s, "test_container"))
        .unwrap_or_else(|| panic!("expected When() container; got {syms:?}"));
    assert_eq!(when_sym.kind, SymbolKind::Function);

    let then_sym = syms
        .iter()
        .find(|s| s.name == "should return the correct sum" && meta_bool(s, "is_test"))
        .unwrap_or_else(|| panic!("expected then() test; got {syms:?}"));
    assert_eq!(then_sym.kind, SymbolKind::Function);
    // then must not also be a container
    assert!(!meta_bool(then_sym, "test_container"));
}

// ---------------------------------------------------------------------------
// Kotest lifecycle hooks
// ---------------------------------------------------------------------------

#[test]
fn kotest_lifecycle_hooks_are_lifecycle() {
    // `beforeEach { }` / `afterAll { }` → is_test + test_lifecycle.
    let code = r#"class LifecycleSpec : DescribeSpec({
  beforeEach {
    println("setup")
  }
  afterAll {
    println("teardown")
  }
  describe("something") {
    it("works") { }
  }
})
"#;
    let syms = symbols(code, "src/test/kotlin/LifecycleSpec.kt");

    let before = syms
        .iter()
        .find(|s| s.name == "beforeEach")
        .unwrap_or_else(|| panic!("expected beforeEach lifecycle symbol; got {syms:?}"));
    assert!(meta_bool(before, "is_test"), "lifecycle must set is_test");
    assert!(
        meta_bool(before, "test_lifecycle"),
        "lifecycle must set test_lifecycle"
    );

    let after = syms
        .iter()
        .find(|s| s.name == "afterAll")
        .unwrap_or_else(|| panic!("expected afterAll lifecycle symbol; got {syms:?}"));
    assert!(meta_bool(after, "is_test"), "lifecycle must set is_test");
    assert!(
        meta_bool(after, "test_lifecycle"),
        "lifecycle must set test_lifecycle"
    );
}

// ---------------------------------------------------------------------------
// Spek
// ---------------------------------------------------------------------------

#[test]
fn spek_describe_and_it() {
    // Spek `describe("…") { it("…") { } }` — same DSL as Kotest DescribeSpec.
    let code = r#"class CalculatorSpec : Spek({
  describe("a calculator") {
    it("returns the sum of its arguments") {
      val calculator = Calculator()
      assertEquals(4, calculator.sum(2, 2))
    }
  }
})
"#;
    let syms = symbols(code, "src/test/kotlin/CalculatorSpec.kt");
    let d = syms
        .iter()
        .find(|s| s.name == "a calculator" && meta_bool(s, "test_container"))
        .unwrap_or_else(|| panic!("expected Spek describe() container; got {syms:?}"));
    assert_eq!(d.kind, SymbolKind::Function);
    let t = syms
        .iter()
        .find(|s| s.name == "returns the sum of its arguments" && meta_bool(s, "is_test"))
        .unwrap_or_else(|| panic!("expected Spek it() test; got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
}

#[test]
fn spek_beforeeachtest_is_lifecycle() {
    // Spek `beforeEachTest { }` → is_test + test_lifecycle.
    let code = r#"class SetupSpec : Spek({
  describe("setup") {
    beforeEachTest {
      initDb()
    }
    it("runs after setup") { }
  }
})
"#;
    let syms = symbols(code, "src/test/kotlin/SetupSpec.kt");
    let lc = syms
        .iter()
        .find(|s| s.name == "beforeEachTest")
        .unwrap_or_else(|| panic!("expected beforeEachTest lifecycle; got {syms:?}"));
    assert!(meta_bool(lc, "is_test"));
    assert!(meta_bool(lc, "test_lifecycle"));
}

// ---------------------------------------------------------------------------
// Negative control
// ---------------------------------------------------------------------------

#[test]
fn non_test_calls_not_materialized() {
    // Guards exercised:
    //   - `describe("x")` used as a plain return value (no trailing lambda) →
    //     trailing-lambda guard rejects it.
    //   - `println("hello")` — vocab guard rejects it.
    //   - `someAction("label") { run() }` — arbitrary lambda call, callee not in vocab.
    //   - `assert(x == y)` — assertion, no trailing lambda and not in vocab.
    // None may produce a test symbol or test-role metadata.
    let code = r#"object Demo {
    fun produceLabel(): String {
        return describe("my-api")
    }

    fun run() {
        println("hello world")
        someAction("label") {
            doWork()
        }
        assert(1 == 1)
    }
}
"#;
    let syms = symbols(code, "src/main/kotlin/Demo.kt");
    assert!(
        !syms
            .iter()
            .any(|s| s.name == "my-api" || s.name == "hello world" || s.name == "label"),
        "non-DSL calls must not materialize test symbols; got {syms:?}"
    );
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "no test-role metadata should appear for production code; got {syms:?}"
    );
}

#[test]
fn qualified_vocab_callee_not_materialized() {
    // Regression lock for the `navigation_expression` false-positive.
    //
    // `it` is the single most dangerous word: it IS in the test vocab AND it is
    // the standard implicit lambda-parameter name in Kotlin.  In production code:
    //
    //   list.forEach {
    //     it.register("widget") { configure() }   // it = lambda param; .register = builder call
    //   }
    //
    // `it.register("widget") { }` parses as a CURRIED call_expression whose inner
    // call has a `navigation_expression` callee (not a bare `identifier`).  The
    // leading segment "it" IS in the test vocab — so without the bare-identifier
    // guard, `classify_call("it.register", …)` would fire (split('.').next()=="it")
    // and materialize a bogus test symbol named "widget".
    //
    // `obj.describe("x") { }` exercises the same path for a container-vocab word
    // used as a method name on an arbitrary receiver — must also be rejected.
    let code = r#"class WidgetRegistry {
    fun register(items: List<Item>) {
        items.forEach {
            it.register("widget") {
                configure()
            }
        }
        obj.describe("x") {
            doSomething()
        }
    }
}
"#;
    let syms = symbols(code, "src/main/kotlin/WidgetRegistry.kt");
    assert!(
        !syms.iter().any(|s| s.name == "widget"),
        "it.register(\"widget\") must NOT materialize a test symbol; got {syms:?}"
    );
    assert!(
        !syms.iter().any(|s| s.name == "x"),
        "obj.describe(\"x\") must NOT materialize a test symbol; got {syms:?}"
    );
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "qualified vocab-word calls must produce zero test-role metadata; got {syms:?}"
    );
}
