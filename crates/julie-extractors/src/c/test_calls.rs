//! C Criterion call-style test extraction (Miller bridge test-roles).
//!
//! Criterion declares tests with a macro that the C grammar parses as a *call*,
//! not a function definition:
//!
//! ```c
//! Test(math, addition) {
//!     cr_assert(2 + 2 == 4);
//! }
//! ```
//!
//! tree-sitter-c parses `Test(math, addition)` as a `call_expression` (function
//! field = identifier `Test`, arguments = an `argument_list` of two bare
//! `identifier`s — the suite and the test name) followed by a DETACHED
//! `compound_statement` block (the block is a sibling of the call, not a child).
//! Unlike the JS/Dart string-named DSLs, Criterion's name is built from the two
//! identifier arguments joined `suite.name`. Only that grammar walking is
//! C-local; classification and symbol construction delegate to the shared
//! `crate::test_calls` core so the captured `is_test` metadata is identical across
//! languages and the downstream `classify_symbols_by_role` pass treats Criterion
//! like every other call-style framework.
//!
//! Optional trailing arguments (`Test(suite, name, .init = setup)`) are ignored —
//! only the first two identifier arguments form the name.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// Criterion vocabulary. `Test` is the only test-case macro that parses as a
/// call_expression; suite grouping is expressed through the first argument, not a
/// separate container call, so there is no container/lifecycle entry here.
const CRITERION_VOCAB: TestCallVocab = TestCallVocab {
    test: &["Test"],
    container: &[],
    lifecycle: &[],
};

/// Materialize a Criterion `Test(suite, name) { ... }` call as a test symbol.
/// Returns `None` for any call that is not a recognized Criterion test macro
/// (e.g. `cr_assert(...)`, `printf(...)`), so the caller can blindly invoke it for
/// every `call_expression` and only Criterion tests become symbols.
pub fn extract_c_test_call(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }

    let function_node = node.child_by_field_name("function")?;
    let full_callee = base.get_node_text(&function_node);
    // Exact match only (#66): a qualified/member callee (`Test.run(...)`, function
    // field = `field_expression`) never equals the dotless `Test` macro name, so
    // the exact-matcher rejects it without the JS-only leading-segment split.
    let category = classify_call_exact(&full_callee, &CRITERION_VOCAB)?;
    debug_assert_eq!(category, TestCallCategory::Test);

    // Criterion names a test by its first two identifier arguments (suite, name).
    let args_node = node.child_by_field_name("arguments")?;
    let mut cursor = args_node.walk();
    let identifier_args: Vec<String> = args_node
        .children(&mut cursor)
        .filter(|c| c.kind() == "identifier")
        .take(2)
        .map(|c| base.get_node_text(&c))
        .collect();
    if identifier_args.is_empty() {
        return None;
    }
    let name = identifier_args.join(".");

    Some(build_test_call_symbol(
        base,
        node,
        &full_callee,
        name,
        category,
        parent_id,
    ))
}
