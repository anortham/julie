//! PowerShell Pester call-style test extraction (Miller bridge test-roles).
//!
//! Pester tests are call expressions (`command` nodes in the PowerShell grammar),
//! not named function declarations:
//!
//! ```powershell
//! Describe "math module" {
//!     BeforeAll { ... }
//!     It "should add numbers" { 1 + 1 | Should -Be 2 }
//!     Context "nested" {
//!         AfterEach { }
//!     }
//! }
//! ```
//!
//! Grammar shape (confirmed via live AST probe against tree-sitter-powershell
//! rev d398441):
//! - Node kind: `command`
//! - Callee: `command_name` **field** (kind `command_name`) — text is the bare
//!   cmdlet name (`Describe`, `It`, …).
//! - Description arg: first non-separator child of `command_elements` →
//!   `array_literal_expression → unary_expression → string_literal →
//!   expandable_string_literal | verbatim_string_characters`.
//!   Decoded via `base.decode_string_literal`.
//! - Block body: second non-separator child is a `script_block_expression`;
//!   its nested `command` nodes become child symbols in the normal tree walk.
//!
//! Only the grammar walking is PowerShell-local; classification and symbol
//! construction delegate to the shared `crate::test_calls` core so the captured
//! `is_test` / `test_container` / `test_lifecycle` metadata is byte-identical to
//! the JS/TS, Dart, Lua, R, C, and C++ call-style paths.

use crate::base::{BaseExtractor, Symbol};
use crate::test_calls::{
    build_test_call_symbol, classify_call_exact, TestCallCategory, TestCallVocab,
};
use tree_sitter::Node;

/// Pester vocabulary.
/// - `Describe` / `Context` → container (`test_container = true`)
/// - `It` → test case (`is_test = true`)
/// - `BeforeAll` / `AfterAll` / `BeforeEach` / `AfterEach` → lifecycle
///   (`is_test = true` + `test_lifecycle = true`)
const PESTER_VOCAB: TestCallVocab = TestCallVocab {
    test: &["It"],
    container: &["Describe", "Context"],
    lifecycle: &["BeforeAll", "AfterAll", "BeforeEach", "AfterEach"],
};

/// Materialize a Pester `command` as a test/container/lifecycle symbol. Returns
/// `None` for any command that is not a recognized Pester DSL call (e.g.
/// `Write-Host "msg"`, `Get-Date`), so the caller can invoke it for every
/// `command` node and only Pester DSL calls become symbols.
pub(super) fn extract_pester_test_call(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "command" {
        return None;
    }

    // The callee lives in the `command_name` field.
    let callee_node = node.child_by_field_name("command_name")?;
    let full_callee = base.get_node_text(&callee_node);
    // Exact match only (#66): a bareword command name can contain '.' (invoking a
    // script `Context.Helper`), a single dotted token a node-kind guard cannot
    // catch (Mech B). Exact match never equates it to the dotless `Context`. Pester
    // cmdlets are dotless.
    let category = classify_call_exact(&full_callee, &PESTER_VOCAB)?;

    let name = match category {
        // Lifecycle hooks take no description string; use the callee name.
        TestCallCategory::Lifecycle => full_callee.to_string(),
        // Describe / Context / It take the description as the first string in
        // the argument list.
        _ => {
            let elements = node.child_by_field_name("command_elements")?;
            extract_first_string(base, elements)?
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

/// Walk the `command_elements` node's children, skip `command_argument_sep`
/// nodes, and return the decoded string content from the first non-separator
/// child that contains a `string`-kinded descendant.
///
/// The path through the grammar is:
/// `command_elements → array_literal_expression → unary_expression →
/// string_literal → expandable_string_literal | verbatim_string_characters`.
fn extract_first_string(base: &mut BaseExtractor, elements: Node) -> Option<String> {
    let mut cursor = elements.walk();
    for child in elements.children(&mut cursor) {
        if child.kind() == "command_argument_sep" {
            continue;
        }
        // Recursively find the first "string"-kinded node in this subtree.
        if let Some(string_node) = find_string_node(child) {
            return base.decode_string_literal(&string_node);
        }
    }
    None
}

/// Recursively find the first node whose kind contains "string" within the
/// given subtree. Returns as soon as the first matching node is found.
fn find_string_node(node: Node) -> Option<Node> {
    if node.kind().contains("string") {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_string_node(child) {
            return Some(found);
        }
    }
    None
}
