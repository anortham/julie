//! C++ Catch2 call-style test extraction (Miller bridge test-roles).
//!
//! Catch2 declares tests with macros that the C++ grammar parses as *calls*, not
//! function definitions:
//!
//! ```cpp
//! TEST_CASE("vector ops", "[vector]") {
//!     SECTION("push_back grows") {
//!         REQUIRE(v.size() == 1);
//!     }
//! }
//! ```
//!
//! tree-sitter-cpp parses `TEST_CASE("vector ops", "[vector]")` as a
//! `call_expression` (function field = identifier `TEST_CASE`, arguments = an
//! `argument_list` of `string_literal`s) followed by a DETACHED `compound_statement`
//! block (the block is a sibling of the call, not a child — so a nested `SECTION`
//! is NOT an AST child of its `TEST_CASE`, and parent linkage stays flat). The test
//! name is the first `string_literal` argument: this also handles
//! `TEST_CASE_METHOD(Fixture, "name", "[tag]")`, whose first argument is the
//! fixture identifier and whose name is the SECOND argument (the first string).
//!
//! Only the grammar walking is C++-local; classification and symbol construction
//! delegate to the shared `crate::test_calls` core so the captured `is_test` /
//! `test_container` metadata is identical across languages and the downstream
//! `classify_symbols_by_role` pass treats Catch2 like every other call-style
//! framework.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// Catch2 vocabulary.
/// - Test cases: `TEST_CASE`, `SCENARIO` (BDD), `TEST_CASE_METHOD` (fixture).
/// - Containers: `SECTION` and the BDD section aliases `GIVEN`/`WHEN`/`THEN`.
///
/// Catch2 has no call-style lifecycle hooks (fixtures are classes used via
/// `TEST_CASE_METHOD`), so the lifecycle set is empty.
const CATCH2_VOCAB: TestCallVocab = TestCallVocab {
    test: &["TEST_CASE", "SCENARIO", "TEST_CASE_METHOD"],
    container: &["SECTION", "GIVEN", "WHEN", "THEN"],
    lifecycle: &[],
};

/// Materialize a Catch2 `TEST_CASE("name", ...) { ... }` (or `SECTION`, `SCENARIO`,
/// …) call as a test / container symbol. Returns `None` for any call that is not a
/// recognized Catch2 macro (e.g. `REQUIRE(...)`, `CHECK(...)`), so the caller can
/// blindly invoke it for every `call_expression` and only Catch2 DSL calls become
/// symbols.
pub fn extract_cpp_test_call(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }

    let function_node = node.child_by_field_name("function")?;
    let full_callee = base.get_node_text(&function_node);
    // Exact match only (#66): a qualified/member callee (`TEST_CASE.configure(...)`,
    // function field = `field_expression`) never equals a dotless Catch2 macro
    // name, so the exact-matcher rejects it without the JS-only leading split.
    let category = classify_call_exact(&full_callee, &CATCH2_VOCAB)?;
    // Catch2 has no call-style lifecycle hooks; every recognized macro names itself
    // with its first string-literal argument.
    debug_assert_ne!(category, TestCallCategory::Lifecycle);

    // The display name is the FIRST string-literal argument — works for TEST_CASE /
    // SECTION / SCENARIO (1st arg) and TEST_CASE_METHOD (fixture identifier first,
    // name second).
    let args_node = node.child_by_field_name("arguments")?;
    let mut cursor = args_node.walk();
    let first_string = args_node
        .children(&mut cursor)
        .find(|c| c.kind() == "string_literal")?;
    let name = base.decode_string_literal(&first_string)?;

    Some(build_test_call_symbol(
        base,
        node,
        &full_callee,
        name,
        category,
        parent_id,
    ))
}
