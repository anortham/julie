//! Swift Quick/Nimble call-style test extraction (Miller bridge test-roles).
//!
//! Quick and Nimble declare tests as call expressions (`call_expression` nodes
//! in the Swift grammar), not named function declarations:
//!
//! ```swift
//! describe("math module") {
//!     context("addition") {
//!         beforeEach { }
//!         afterEach  { }
//!         it("should add two numbers") {
//!             expect(1 + 1).to(equal(2))
//!         }
//!     }
//! }
//! ```
//!
//! Grammar shape (confirmed via live AST probe against tree-sitter-swift):
//! - Node kind: `call_expression`
//! - Callee: first `simple_identifier` child (e.g. `"describe"`, `"it"`)
//! - Description string: `call_suffix` → `value_arguments` → first named child
//!   (`value_argument`) → `value` field → `line_string_literal`. Decoded via
//!   `base.decode_string_literal`.
//! - Lifecycle calls (`beforeEach`, `afterEach`, `beforeAll`, `afterAll`,
//!   `justBeforeEach`) carry no description arg — only a trailing closure in
//!   `call_suffix`. The callee name is used as the symbol name.
//!
//! Focused / pending variants (`fit`, `xit`, `fdescribe`, `xdescribe`, …) are
//! included so xcodebuild focus / skip modifiers are still materialised.
//!
//! XCTest (`class … : XCTestCase`) and Swift Testing (`@Test` / `@Suite`) are
//! handled separately by the base-type classifier and annotation extraction in
//! task #48. This adapter is purely additive.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// Quick/Nimble vocabulary.
/// - `describe` / `context` (+ focused/excluded variants) → container
/// - `it` / `specify` (+ focused/excluded variants) → test case
/// - `beforeEach` / `afterEach` / `beforeAll` / `afterAll` / `justBeforeEach` → lifecycle
const QUICK_VOCAB: TestCallVocab = TestCallVocab {
    test: &["it", "xit", "fit", "specify", "xspecify", "fspecify", "pending"],
    container: &[
        "describe",
        "xdescribe",
        "fdescribe",
        "context",
        "xcontext",
        "fcontext",
    ],
    lifecycle: &[
        "beforeEach",
        "afterEach",
        "beforeAll",
        "afterAll",
        "justBeforeEach",
    ],
};

/// Materialize a Quick/Nimble `call_expression` as a test/container/lifecycle
/// symbol. Returns `None` for any call that is not a recognised Quick DSL call
/// (e.g. `URL(string: "…")`, `print("…")`), so the caller can invoke this for
/// every `call_expression` node and only DSL calls become symbols.
pub(super) fn extract_quick_test_call(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }

    // Callee: first `simple_identifier` child of the call_expression.
    let callee_node = {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find(|c| c.kind() == "simple_identifier")?
    };
    let full_callee = base.get_node_text(&callee_node);
    // Exact match only (#66): the callee is already the first DIRECT
    // `simple_identifier` child, so a member receiver never reaches here — but use
    // the exact-matcher uniformly so the JS-only `.`-split never applies.
    let category = classify_call_exact(&full_callee, &QUICK_VOCAB)?;

    let name = match category {
        // Lifecycle calls have no description string; use the callee name.
        TestCallCategory::Lifecycle => full_callee.to_string(),
        // describe/context/it/specify — first string argument is the description.
        _ => {
            let call_suffix = {
                let mut c = node.walk();
                node.children(&mut c).find(|n| n.kind() == "call_suffix")?
            };
            let value_args = {
                let mut c = call_suffix.walk();
                call_suffix
                    .children(&mut c)
                    .find(|n| n.kind() == "value_arguments")?
            };
            // First named child is the value_argument wrapper.
            let first_arg = {
                let mut c = value_args.walk();
                value_args.named_children(&mut c).next()?
            };
            // value_argument holds the literal in its `value` field; fall back
            // to the node itself for bare literals without a label.
            let value_node = if first_arg.kind() == "value_argument" {
                first_arg
                    .child_by_field_name("value")
                    .unwrap_or(first_arg)
            } else {
                first_arg
            };
            base.decode_string_literal(&value_node)?
        }
    };

    Some(build_test_call_symbol(
        base,
        &node,
        &full_callee,
        name,
        category,
        parent_id,
    ))
}
