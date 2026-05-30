//! Bash shellspec/bats call-style test detection (Miller bridge test-roles).
//!
//! shellspec declares tests as call expressions with single-quoted arguments:
//!
//! ```bash
//! Describe 'math module'
//!   Context 'addition'
//!     It 'adds two numbers'
//!       When call expr 1 + 1
//!       The output should eq 2
//!     End
//!   End
//! End
//! ```
//!
//! bats uses a specialized `@test "description" { }` form:
//!
//! ```bash
//! @test "adds two numbers" {
//!     result="$(expr 1 + 1)"
//!     [ "$result" -eq 2 ]
//! }
//! ```
//!
//! Grammar shape (confirmed via live AST probe against tree-sitter-bash 0.25.1):
//! - Node kind: `command`
//! - Callee: `name` **field** (kind `command_name` → inner `word`) — text is the
//!   bare command name (`Describe`, `It`, `@test`, …).
//! - Description arg: first `argument` field child whose kind contains `"string"` —
//!   `raw_string` for shellspec single-quoted args, `string` for bats double-quoted
//!   args. Decoded via `base.decode_string_literal`.
//!
//! Notes on `@test`: bats `@test "name" { }` parses as a `command` node (not a
//! `function_definition`), with the `{` as a trailing `word` argument. No nested
//! DSL calls exist inside a bats test body, so parent_id propagation is correct.
//!
//! `setup()`/`teardown()` are `function_definition` nodes, not commands; they are
//! extracted by the function extraction path and receive `is_test = true` via the
//! `classify_symbols_by_role` name-heuristic pass. The call-style adapter therefore
//! carries no lifecycle vocabulary.

use crate::base::Symbol;
use crate::bash::BashExtractor;
use std::path::PathBuf;

fn symbols(code: &str) -> Vec<Symbol> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .expect("load Bash grammar");
    let tree = parser.parse(code, None).expect("parse Bash");
    let mut ext = BashExtractor::new(
        "bash".to_string(),
        "test.bats".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    ext.extract_symbols(&tree)
}

fn meta_bool(s: &Symbol, key: &str) -> bool {
    s.metadata
        .as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn bash_qualified_callee_is_not_materialized() {
    // FALSE-POSITIVE GUARD (#66): `classify_call` keys on the segment before the
    // first '.'. A bash command name is a `word` that can contain '.' (e.g.
    // invoking a script `It.helper`), which would otherwise be misclassified as a
    // shellspec `It`. shellspec/bats DSL keywords are always dotless, so a dotted
    // command name must never materialize a test symbol.
    let code = r#"
It.helper 'config value'
"#;
    let syms = symbols(code);
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "dotted command name `It.helper` must not materialize a test symbol, got {syms:?}"
    );
}

#[test]
fn shellspec_describe_context_it_emit_test_role_metadata() {
    let code = r#"
Describe 'math module'
  Context 'addition'
    It 'adds two numbers'
      When call expr 1 + 1
      The output should eq 2
    End
  End
End
"#;
    let syms = symbols(code);

    // Describe → test_container
    let desc = syms
        .iter()
        .find(|s| s.name == "math module")
        .unwrap_or_else(|| panic!("expected a Describe container symbol, got {syms:?}"));
    assert!(
        meta_bool(desc, "test_container"),
        "Describe is a test container"
    );
    assert!(
        !meta_bool(desc, "is_test"),
        "a container is not itself a test case"
    );

    // Context → test_container
    let ctx = syms
        .iter()
        .find(|s| s.name == "addition")
        .unwrap_or_else(|| panic!("expected a Context container symbol, got {syms:?}"));
    assert!(
        meta_bool(ctx, "test_container"),
        "Context is a test container"
    );

    // It → is_test
    let it = syms
        .iter()
        .find(|s| s.name == "adds two numbers")
        .unwrap_or_else(|| panic!("expected an It test symbol, got {syms:?}"));
    assert!(meta_bool(it, "is_test"), "It is a test case");
}

#[test]
fn bats_at_test_emits_test_role_metadata() {
    let code = r#"
@test "adds two numbers" {
    result="$(expr 1 + 1)"
    [ "$result" -eq 2 ]
}
@test "subtracts numbers" {
    result="$(expr 5 - 3)"
    [ "$result" -eq 2 ]
}
"#;
    let syms = symbols(code);

    let first = syms
        .iter()
        .find(|s| s.name == "adds two numbers")
        .unwrap_or_else(|| panic!("expected an @test symbol, got {syms:?}"));
    assert!(meta_bool(first, "is_test"), "@test carries is_test");

    let second = syms
        .iter()
        .find(|s| s.name == "subtracts numbers")
        .unwrap_or_else(|| panic!("expected second @test symbol, got {syms:?}"));
    assert!(meta_bool(second, "is_test"), "second @test carries is_test");
}

#[test]
fn non_dsl_bash_commands_do_not_become_test_symbols() {
    // echo and curl are plain Bash commands — must not carry test-role metadata.
    let code = r#"
echo "not a test"
curl "https://example.com/api"
"#;
    let syms = symbols(code);
    assert_eq!(
        syms.iter()
            .filter(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container"))
            .count(),
        0,
        "plain Bash commands must not carry test-role metadata: {syms:?}"
    );
}
