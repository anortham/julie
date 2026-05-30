//! Lua busted call-style test extraction (Miller bridge test-roles).
//!
//! Like JS/TS (Jest/Vitest) and Dart (`package:test`), busted tests are call
//! expressions, not named function declarations:
//!
//! ```lua
//! describe("math", function()
//!   before_each(function() end)
//!   it("adds", function() assert.equal(2, 1 + 1) end)
//! end)
//! ```
//!
//! The grammar shape is `function_call` with a `name` field (the callee — a bare
//! `identifier` for DSL calls; a `dot_index_expression` such as `assert.equal`
//! for method calls, which classify to `None`) and an `arguments` field whose
//! first `string` child carries the description. Only the grammar walking is
//! Lua-local; classification and symbol construction delegate to the shared
//! `crate::test_calls` core so the captured `is_test` / `test_container` /
//! `test_lifecycle` metadata is byte-identical to the JS/TS and Dart paths and
//! the downstream `classify_symbols_by_role` pass treats them identically.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// busted vocabulary. `it` is a case, `describe`/`context` are containers, and
/// `before_each`/`after_each`/`setup`/`teardown`/`lazy_setup`/`lazy_teardown`
/// are lifecycle fixtures.
const LUA_VOCAB: TestCallVocab = TestCallVocab {
    test: &["it"],
    container: &["describe", "context"],
    lifecycle: &[
        "before_each",
        "after_each",
        "setup",
        "teardown",
        "lazy_setup",
        "lazy_teardown",
    ],
};

/// Materialize a busted `function_call` as a test/container/lifecycle symbol.
/// Returns `None` for any call that is not a recognized busted DSL call (e.g.
/// `assert.equal(...)`, `require(...)`, `print(...)`), so the caller can invoke
/// it for every `function_call` and only DSL calls become symbols.
pub(super) fn extract_lua_test_call(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "function_call" {
        return None;
    }

    let callee_node = node.child_by_field_name("name")?;
    let full_callee = base.get_node_text(&callee_node);
    // Exact match only (#66): a dot-index method call (`it.register(...)`, name
    // field = `dot_index_expression`) never equals a dotless busted DSL name, so
    // the exact-matcher rejects it without the JS-only leading-segment split.
    let category = classify_call_exact(&full_callee, &LUA_VOCAB)?;

    let name = match category {
        // Lifecycle calls take no description string; use the callee's base name.
        TestCallCategory::Lifecycle => full_callee
            .split('.')
            .next()
            .unwrap_or(&full_callee)
            .to_string(),
        // it/describe/context take the description as the first string argument.
        _ => {
            let args_node = node.child_by_field_name("arguments")?;
            let mut cursor = args_node.walk();
            let first_string = args_node
                .children(&mut cursor)
                .find(|c| c.kind() == "string")?;
            base.decode_string_literal(&first_string)?
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
