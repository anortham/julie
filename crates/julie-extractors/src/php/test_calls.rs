//! PHP Pest call-style test extraction (Miller bridge test-roles).
//!
//! Like Lua (busted) and R (testthat), Pest tests are call expressions, not
//! named function declarations:
//!
//! ```php
//! describe('User management', function () {
//!     beforeEach(function () { /* setup */ });
//!     it('can create a user', function () { expect(true)->toBeTrue(); });
//!     test('computes totals', function () { expect(1 + 1)->toBe(2); });
//! });
//! ```
//!
//! The grammar shape is `function_call_expression` with a `function` field
//! (the callee — a bare `name` node for DSL calls; member-access expressions
//! for method calls, which classify to `None`) and an `arguments` field whose
//! first `argument` child carries the description as a `string` (single-quoted)
//! or `encapsed_string` (double-quoted) node.
//!
//! Only the grammar walking is PHP-local; classification and symbol construction
//! delegate to the shared `crate::test_calls` core so the captured `is_test` /
//! `test_container` / `test_lifecycle` metadata is byte-identical to the other
//! call-style paths (Lua, R, JS/TS, Dart) and the downstream
//! `classify_symbols_by_role` pass treats them identically.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// Pest vocabulary.  `test` and `it` are test cases; `describe` is a
/// container; `beforeEach`/`afterEach`/`beforeAll`/`afterAll` are lifecycle
/// fixtures (Pest's full hook set, confirmed present in the tree-sitter-php
/// grammar via `function_call_expression` with a bare `name` callee).
const PHP_PEST_VOCAB: TestCallVocab = TestCallVocab {
    test: &["test", "it"],
    container: &["describe"],
    lifecycle: &["beforeEach", "afterEach", "beforeAll", "afterAll"],
};

/// Materialise a Pest `function_call_expression` as a test/container/lifecycle
/// symbol.  Returns `None` for any call that is not a recognised Pest DSL call
/// (e.g. `expect(...)` matchers, `array_map(...)`, `new Foo(...)`) so the
/// caller can invoke it for every `function_call_expression` node and only DSL
/// calls become symbols.
pub(super) fn extract_php_pest_test_call(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "function_call_expression" {
        return None;
    }

    // `function` field holds the callee.  For plain Pest DSL calls it is a
    // bare `name` node ("test", "it", "describe", …).  For method calls
    // (e.g. `$this->helper(...)`) the callee kind differs (member/scoped call)
    // and never reaches this `function_call_expression` arm, so those are filtered
    // out automatically.
    let callee_node = node.child_by_field_name("function")?;
    let full_callee = base.get_node_text(&callee_node);
    // Exact match only (#66): use the exact-matcher uniformly across non-JS
    // adapters so the JS-only leading-`.`-split never applies (PHP names are
    // dotless, so this is behaviour-neutral here, but keeps the contract uniform).
    let category = classify_call_exact(&full_callee, &PHP_PEST_VOCAB)?;

    let name = match category {
        // Lifecycle hooks take no description string; use the callee name.
        TestCallCategory::Lifecycle => full_callee
            .split('.')
            .next()
            .unwrap_or(&full_callee)
            .to_string(),
        // test/it/describe take the description as the first argument's string
        // value.  The `string` (single-quoted) or `encapsed_string`
        // (double-quoted) lives as a direct child of the first `argument` node
        // inside the `arguments` wrapper.
        _ => {
            let args_node = node.child_by_field_name("arguments")?;
            let mut cursor = args_node.walk();
            let first_arg = args_node
                .children(&mut cursor)
                .find(|c| c.kind() == "argument")?;
            let mut inner = first_arg.walk();
            let string_node = first_arg
                .children(&mut inner)
                .find(|c| c.kind().contains("string"))?;
            base.decode_string_literal(&string_node)?
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
