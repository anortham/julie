//! PowerShell Pester call-style test detection (Miller bridge test-roles).
//!
//! Pester declares tests as call expressions:
//! ```powershell
//! Describe "module tests" {
//!     BeforeAll { $x = setup }
//!     It "should add" { 1 + 1 | Should -Be 2 }
//!     Context "nested" {
//!         AfterEach { }
//!     }
//! }
//! ```
//!
//! Grammar: each line is a `command` node — `command_name` holds the callee
//! (`Describe`, `It`, etc.) and `command_elements` holds the arguments; the
//! description string is the first `string_literal`/`expandable_string_literal`
//! in the argument list.
//!
//! These tests assert canonical `is_test` / `test_container` / `test_lifecycle`
//! metadata on extracted symbols and confirm that non-Pester commands
//! (`Write-Host "x"`) do NOT become test symbols.

use crate::base::Symbol;
use crate::powershell::PowerShellExtractor;
use std::path::PathBuf;

fn symbols(code: &str) -> Vec<Symbol> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_powershell::LANGUAGE.into())
        .expect("load PowerShell grammar");
    let tree = parser.parse(code, None).expect("parse PowerShell");
    let mut ext = PowerShellExtractor::new(
        "powershell".to_string(),
        "Test.Tests.ps1".to_string(),
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
fn powershell_qualified_callee_is_not_materialized() {
    // FALSE-POSITIVE GUARD (#66): `classify_call` keys on the segment before the
    // first '.'. A bareword command name can contain '.' (e.g. invoking a script
    // `Context.Helper`), which would otherwise be misclassified as a Pester
    // `Context`. Pester DSL cmdlets are always dotless, so a dotted command name
    // must never materialize a test symbol.
    let code = r#"
Context.Helper "config" {
    Get-Item
}
"#;
    let syms = symbols(code);
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "dotted command name `Context.Helper` must not materialize a test symbol, got {syms:?}"
    );
}

#[test]
fn pester_describe_it_context_lifecycle_emit_test_role_metadata() {
    let code = r#"
Describe "math module" {
    BeforeAll { $x = 1 }
    AfterAll { }
    Context "addition" {
        BeforeEach { }
        AfterEach { }
        It "should add two numbers" {
            1 + 1 | Should -Be 2
        }
    }
}
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
        .find(|s| s.name == "should add two numbers")
        .unwrap_or_else(|| panic!("expected an It test symbol, got {syms:?}"));
    assert!(meta_bool(it, "is_test"), "It is a test case");

    // BeforeAll → is_test + test_lifecycle
    let before_all = syms
        .iter()
        .find(|s| s.name == "BeforeAll")
        .unwrap_or_else(|| panic!("expected a BeforeAll lifecycle symbol, got {syms:?}"));
    assert!(
        meta_bool(before_all, "is_test"),
        "lifecycle hook carries is_test"
    );
    assert!(
        meta_bool(before_all, "test_lifecycle"),
        "BeforeAll is a lifecycle hook"
    );

    // AfterAll → is_test + test_lifecycle
    let after_all = syms
        .iter()
        .find(|s| s.name == "AfterAll")
        .unwrap_or_else(|| panic!("expected an AfterAll lifecycle symbol, got {syms:?}"));
    assert!(
        meta_bool(after_all, "is_test"),
        "AfterAll carries is_test"
    );
    assert!(
        meta_bool(after_all, "test_lifecycle"),
        "AfterAll is a lifecycle hook"
    );

    // BeforeEach + AfterEach → lifecycle
    let before_each = syms
        .iter()
        .find(|s| s.name == "BeforeEach")
        .unwrap_or_else(|| panic!("expected a BeforeEach lifecycle symbol, got {syms:?}"));
    assert!(meta_bool(before_each, "test_lifecycle"), "BeforeEach is lifecycle");

    let after_each = syms
        .iter()
        .find(|s| s.name == "AfterEach")
        .unwrap_or_else(|| panic!("expected an AfterEach lifecycle symbol, got {syms:?}"));
    assert!(meta_bool(after_each, "test_lifecycle"), "AfterEach is lifecycle");
}

#[test]
fn non_pester_commands_do_not_become_test_symbols() {
    // Write-Host and Get-Date are plain PowerShell commands — must not produce
    // test-role metadata even though they contain string arguments.
    let code = r#"
Write-Host "not a test"
Get-Date -Format "yyyy-MM-dd"
"#;
    let syms = symbols(code);
    assert_eq!(
        syms.iter()
            .filter(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container"))
            .count(),
        0,
        "plain PowerShell commands must not carry test-role metadata: {syms:?}"
    );
}
