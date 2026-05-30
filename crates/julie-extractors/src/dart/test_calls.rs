//! Dart `package:test` call-style test extraction (Miller bridge test-roles).
//!
//! Like JS/TS (Jest/Vitest), Dart tests are call expressions, not named function
//! declarations:
//!
//! ```dart
//! void main() {
//!   group('math', () {
//!     setUp(() { ... });
//!     test('adds', () { expect(2 + 2, 4); });
//!   });
//! }
//! ```
//!
//! The grammar shape mirrors TypeScript — `call_expression` with a `function`
//! field (the callee `identifier`) and an `arguments` field — but the string
//! name node is `string_literal` (not JS's `string`/`template_string`) and the
//! vocabulary differs (`group`, `setUp`/`tearDown[All]`, `testWidgets`). Only the
//! grammar walking (string-literal kind, `decode_string_literal`) is Dart-local;
//! classification and symbol construction delegate to the shared
//! `crate::test_calls` core so the captured `is_test` / `test_container` /
//! `test_lifecycle` metadata is byte-identical to the JS/TS path and the
//! downstream `classify_symbols_by_role` pass treats Dart and JS identically.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// Dart `package:test` vocabulary. `test`/`testWidgets` are cases, `group` is a
/// container, `setUp`/`tearDown[All]` are lifecycle fixtures.
const DART_VOCAB: TestCallVocab = TestCallVocab {
    test: &["test", "testWidgets"],
    container: &["group"],
    lifecycle: &["setUp", "tearDown", "setUpAll", "tearDownAll"],
};

/// Materialize a `package:test` call expression as a test/container/lifecycle
/// symbol. Returns `None` for any call that is not a recognized test runner
/// call (e.g. `expect(...)`, `print(...)`), so the caller can blindly invoke it
/// for every `call_expression` and only test DSL calls become symbols.
pub fn extract_dart_test_call(
    base: &mut BaseExtractor,
    node: &Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }

    let function_node = node.child_by_field_name("function")?;
    let full_callee = base.get_node_text(&function_node);
    // Exact match only (#66): a member call (`test.configure(...)`, function field
    // text "test.configure") never equals a dotless package:test DSL name, so the
    // exact-matcher rejects it without the JS-only leading-segment split.
    let category = classify_call_exact(&full_callee, &DART_VOCAB)?;

    let name = match category {
        // Lifecycle calls take no name string; use the callee's base name.
        TestCallCategory::Lifecycle => full_callee
            .split('.')
            .next()
            .unwrap_or(&full_callee)
            .to_string(),
        // test/group take the description as the first string-literal argument.
        _ => {
            let args_node = node.child_by_field_name("arguments")?;
            let mut cursor = args_node.walk();
            let first_string = args_node
                .children(&mut cursor)
                .find(|c| c.kind() == "string_literal")?;
            base.decode_string_literal(&first_string)?
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
