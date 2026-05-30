//! Bash shellspec/bats call-style test extraction (Miller bridge test-roles).
//!
//! shellspec and bats declare tests as call expressions (`command` nodes in the
//! Bash grammar), not named function declarations:
//!
//! ```bash
//! # shellspec
//! Describe 'math module'
//!   Context 'addition'
//!     It 'adds two numbers'
//!       When call expr 1 + 1
//!       The output should eq 2
//!     End
//!   End
//! End
//!
//! # bats
//! @test "adds two numbers" {
//!     result="$(expr 1 + 1)"
//!     [ "$result" -eq 2 ]
//! }
//! ```
//!
//! Grammar shape (confirmed via live AST probe against tree-sitter-bash 0.25.1):
//! - Node kind: `command`
//! - Callee: `name` **field** (kind `command_name`; text is the bare name,
//!   e.g. `"Describe"` or `"@test"`).
//! - Description arg: first `argument` **field** child whose kind contains
//!   `"string"` — `raw_string` for shellspec single-quoted args
//!   (`'math module'`), `string` for bats double-quoted args
//!   (`"adds two numbers"`). Decoded via `base.decode_string_literal`.
//!
//! Notes on `@test`: bats `@test "name" { }` parses as a `command` node. The
//! `{` is a trailing `word` argument; we stop at the first string argument.
//!
//! Lifecycle note: shellspec's `setup()`/`teardown()` are `function_definition`
//! nodes, not commands; they receive `is_test = true` via the
//! `classify_symbols_by_role` name-heuristic pass and do not need separate
//! materialization here. The lifecycle slice is therefore empty.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// shellspec + bats vocabulary.
/// - `Describe` / `Context` → container (`test_container = true`)
/// - `It` / `Specify` / `Example` / `Feature` / `Scenario` → test case (`is_test = true`)
/// - `@test` (bats) → test case (`is_test = true`)
const BASH_VOCAB: TestCallVocab = TestCallVocab {
    test: &["It", "Specify", "Example", "Feature", "Scenario", "@test"],
    container: &["Describe", "Context"],
    lifecycle: &[], // setup/teardown are function_definitions, handled by name-heuristics
};

/// Materialize a shellspec/bats `command` as a test/container symbol. Returns
/// `None` for any command that is not a recognized shellspec or bats DSL call
/// (e.g. `echo "msg"`, `curl "url"`), so the caller can invoke it for every
/// `command` node and only DSL calls become symbols.
pub(super) fn extract_bash_test_call(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "command" {
        return None;
    }

    // The callee lives in the `name` field of the `command` node.
    let callee_node = node.child_by_field_name("name")?;
    let full_callee = base.get_node_text(&callee_node);
    // Exact match only (#66): a bash command name is a `word` that can contain '.'
    // (invoking a script `It.helper`), a single dotted token a node-kind guard
    // cannot catch (Mech B). Exact match never equates it to the dotless `It`.
    // shellspec/bats keywords (`Describe`, `It`, `@test`, …) are dotless.
    let category = classify_call_exact(&full_callee, &BASH_VOCAB)?;

    let name = match category {
        // Lifecycle: no description string; use the callee name.
        // (vocab has no lifecycle entries, but keep the branch uniform.)
        TestCallCategory::Lifecycle => full_callee.to_string(),
        // Describe / Context / It / @test — first string argument is the description.
        _ => {
            let mut cursor = node.walk();
            let first_str = node
                .children_by_field_name("argument", &mut cursor)
                .find(|c| c.kind().contains("string"))?;
            base.decode_string_literal(&first_str)?
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
