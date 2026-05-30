//! Kotlin Kotest / Spek call-style test extraction (Miller bridge, Wave-3).
//!
//! Kotest and Spek express tests as **call expressions**, not named function
//! declarations or class annotations:
//!
//! ```kotlin
//! // Kotest DescribeSpec
//! class CalcSpec : DescribeSpec({
//!   describe("calculator") {
//!     it("adds numbers") { /* … */ }
//!   }
//! })
//!
//! // Kotest FunSpec
//! class FunTests : FunSpec({
//!   test("addition works") { /* … */ }
//!   context("arithmetic") {
//!     test("subtraction") { /* … */ }
//!   }
//! })
//!
//! // Kotest lifecycle
//! class SetupSpec : DescribeSpec({
//!   beforeEach { /* setup */ }
//! })
//! ```
//!
//! The Kotlin grammar (tree-sitter-kotlin-ng, verified against `node-types.json`
//! and an AST probe) uses a **curried pattern** for any call with both a
//! parenthesised argument list and a trailing lambda body:
//!
//! ```text
//! call_expression  [OUTER — the full `describe("name") { }` expression]
//!   call_expression  [INNER — `describe("name")`, the callee + arg clause]
//!     identifier "describe"
//!     value_arguments "(\"name\")"
//!       value_argument
//!         string_literal "name"
//!   annotated_lambda  [the `{ … }` body]
//!     lambda_literal
//!       …
//! ```
//!
//! Lifecycle hooks with no argument take a **simple pattern** (no inner call):
//!
//! ```text
//! call_expression
//!   identifier "beforeEach"
//!   annotated_lambda { … }
//! ```
//!
//! `call_expression` has **no named fields** in the Kotlin grammar — all
//! children are located by kind. Only the grammar walking is Kotlin-local;
//! classification and symbol construction delegate to the shared
//! `crate::test_calls` core so the captured `is_test` / `test_container` /
//! `test_lifecycle` metadata is byte-identical to every other call-style path.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// Kotest / Spek DSL vocabulary.
///
/// **Tests** (`is_test = true`):
/// - `test` — Kotest FunSpec
/// - `it` — Kotest DescribeSpec, BehaviorSpec, Spek
/// - `should` — Kotest ShouldSpec
/// - `then` — Kotest BehaviorSpec (innermost leaf assertion step)
///
/// **Containers** (`test_container = true`):
/// - `describe` — Kotest DescribeSpec, Spek
/// - `context` — Kotest FunSpec / ShouldSpec / Spek
/// - `given` — Kotest BehaviorSpec, Spek BDD form
/// - `When` — Kotest BehaviorSpec intermediate step (capital W: `when` is a
///   reserved Kotlin keyword; Kotest uses `When`)
/// - `and` — Kotest BehaviorSpec continuation step
///
/// **Lifecycle** (`is_test = true` + `test_lifecycle = true`):
/// - `beforeEach` / `afterEach` — Kotest
/// - `beforeAll` / `afterAll` — Kotest
/// - `beforeTest` / `afterTest` — Kotest
/// - `beforeEachTest` / `afterEachTest` — Spek
/// - `beforeGroup` / `afterGroup` — Spek
const KOTLIN_VOCAB: TestCallVocab = TestCallVocab {
    test: &["test", "it", "should", "then"],
    container: &["describe", "context", "given", "When", "and"],
    lifecycle: &[
        "beforeEach",
        "afterEach",
        "beforeAll",
        "afterAll",
        "beforeTest",
        "afterTest",
        "beforeEachTest",
        "afterEachTest",
        "beforeGroup",
        "afterGroup",
    ],
};

/// Materialize a Kotest / Spek `call_expression` as a test/container/lifecycle
/// symbol. Returns `None` for any call that is not a recognized DSL call so the
/// caller can invoke it for every `call_expression` node.
///
/// Handles two grammar patterns (both verified via AST probe):
///
/// 1. **Curried** — `describe("name") { }`, `it("name") { }`, `test("n") { }`:
///    the outer `call_expression`'s first named child is another inner
///    `call_expression` holding the callee identifier + `value_arguments`.
///
/// 2. **Simple** — `beforeEach { }`, `afterAll { }` (lifecycle hooks with no
///    string argument): the outer `call_expression`'s first named child is a
///    plain `identifier`.
///
/// Two guards prevent false positives:
/// 1. **Trailing-lambda guard**: the `call_expression` must have an
///    `annotated_lambda` (or `lambda_literal`) child.  Plain calls like
///    `describe("x")` used as return values and `println(...)` are rejected.
/// 2. **Vocab guard**: the resolved callee name must appear in [`KOTLIN_VOCAB`].
pub(super) fn extract_kotlin_test_call(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }

    // Guard 1: Kotest/Spek DSL calls always have a trailing lambda body.
    let has_trailing_lambda = {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .any(|c| c.kind() == "annotated_lambda" || c.kind() == "lambda_literal")
    };
    if !has_trailing_lambda {
        return None;
    }

    // Determine the grammar pattern from the first named child:
    //   — `call_expression` (inner): curried form `f("name") { }`
    //   — `identifier`            : simple form  `beforeEach { }`
    let first_named = {
        let mut cursor = node.walk();
        node.named_children(&mut cursor).next()
    }?;

    // Callee text and optional string literal from the argument.
    let full_callee: String;
    let string_node: Option<Node>;

    if first_named.kind() == "call_expression" {
        // ── Curried pattern ──────────────────────────────────────────────────
        // OUTER call's first named child is the INNER `f("name")` call.
        let inner = first_named;
        let callee_node = {
            let mut cursor = inner.walk();
            inner.children(&mut cursor).find(|c| {
                matches!(c.kind(), "identifier" | "simple_identifier")
            })?
        };
        full_callee = base.get_node_text(&callee_node);

        // String arg: inner call's value_arguments → first value_argument →
        // first string_literal named child.
        string_node = {
            let va = {
                let mut cursor = inner.walk();
                inner
                    .children(&mut cursor)
                    .find(|c| c.kind() == "value_arguments")
            };
            va.and_then(|va_node| {
                let mut va_cursor = va_node.walk();
                let first_arg = va_node.named_children(&mut va_cursor).next()?;
                let mut arg_cursor = first_arg.walk();
                first_arg
                    .named_children(&mut arg_cursor)
                    .find(|c| c.kind() == "string_literal")
            })
        };
    } else if matches!(first_named.kind(), "identifier" | "simple_identifier") {
        // ── Simple pattern ───────────────────────────────────────────────────
        // Direct identifier callee: lifecycle hooks like `beforeEach { }`.
        full_callee = base.get_node_text(&first_named);
        string_node = None;
    } else {
        return None;
    }

    // Guard 2: classify exactly (#66). The direct-child identifier extraction above
    // already prevents a member receiver from being used as the callee; the
    // exact-matcher keeps the JS-only `.`-split out of every non-JS adapter.
    let category = classify_call_exact(&full_callee, &KOTLIN_VOCAB)?;

    let name = match category {
        // Lifecycle hooks: use the callee base name directly (no description arg).
        TestCallCategory::Lifecycle => full_callee
            .split('.')
            .next()
            .unwrap_or(&full_callee)
            .to_string(),
        // test / container: first string_literal from value_arguments.
        _ => {
            let snode = string_node?;
            base.decode_string_literal(&snode)?
        }
    };

    Some(build_test_call_symbol(
        base,
        node,
        &full_callee,
        name,
        category,
        parent_id,
    ))
}
