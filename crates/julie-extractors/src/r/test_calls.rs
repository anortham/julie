//! R testthat call-style test extraction (Miller bridge test-roles).
//!
//! Like JS/TS, Dart, and Lua busted, testthat tests are call expressions, not
//! named function declarations — both the classic and BDD forms:
//!
//! ```r
//! test_that("math works", { expect_equal(1 + 1, 2) })
//! describe("a widget", { it("renders", { expect_true(TRUE) }) })
//! ```
//!
//! The grammar shape is `call` with a `function` field (the callee identifier)
//! and an `arguments` field whose first `argument` carries the description
//! `string` (`string` -> `string_content`). testthat has no per-call lifecycle
//! DSL (fixtures use `setup()` / withr), so the lifecycle vocabulary is empty.
//! Only the grammar walking is R-local; classification and symbol construction
//! delegate to the shared `crate::test_calls` core so the captured `is_test` /
//! `test_container` metadata is byte-identical to the other call-style paths.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    TestCallCategory, TestCallVocab, build_test_call_symbol, classify_call_exact,
};
use tree_sitter::Node;

/// testthat vocabulary. `test_that` and BDD-style `it` are cases; `describe` is
/// a container. No per-call lifecycle DSL, so the lifecycle slice is empty.
const R_VOCAB: TestCallVocab = TestCallVocab {
    test: &["test_that", "it"],
    container: &["describe"],
    lifecycle: &[],
};

/// Materialize a testthat `call` as a test/container symbol. Returns `None` for
/// any call that is not a recognized testthat DSL call (e.g. `expect_equal(...)`,
/// `library(...)`), so the caller can invoke it for every `call` node and only
/// DSL calls become symbols.
pub(super) fn extract_r_test_call(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call" {
        return None;
    }

    let callee_node = node.child_by_field_name("function")?;
    let full_callee = base.get_node_text(&callee_node);
    // Exact match only (#66): in R '.' is a normal identifier char (S3 dispatch
    // names like `print.data.frame`), NOT a member operator — so `describe.default`
    // is a single dotted `identifier`. Exact match never equates it to the dotless
    // `describe`, closing the Mech-B vector a node-kind guard cannot (it IS a bare
    // identifier). testthat DSL names are dotless.
    let category = classify_call_exact(&full_callee, &R_VOCAB)?;

    let name = match category {
        // Defensive: no lifecycle vocab today, but keep the builder uniform.
        TestCallCategory::Lifecycle => full_callee
            .split('.')
            .next()
            .unwrap_or(&full_callee)
            .to_string(),
        // test_that/describe/it take the description as the first `argument`'s
        // string value. The `string` lives either under the `value` field or as
        // a direct child of the `argument` wrapper, depending on grammar version.
        _ => {
            let args_node = node.child_by_field_name("arguments")?;
            let mut cursor = args_node.walk();
            let first_arg = args_node
                .children(&mut cursor)
                .find(|c| c.kind() == "argument")?;
            let string_node = first_arg
                .child_by_field_name("value")
                .filter(|n| n.kind().contains("string"))
                .or_else(|| {
                    let mut inner = first_arg.walk();
                    first_arg
                        .children(&mut inner)
                        .find(|c| c.kind().contains("string"))
                })?;
            base.decode_string_literal(&string_node)?
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
