//! Go Ginkgo call-style test detection (Miller bridge test-roles, Wave-3).
//!
//! Ginkgo declares tests as call expressions, not named function declarations:
//!
//! ```go
//! var _ = Describe("math", func() {
//!     Context("addition", func() {
//!         BeforeEach(func() { })
//!         AfterEach(func() { })
//!         It("should add two numbers", func() {
//!             Expect(1 + 1).To(Equal(2))
//!         })
//!     })
//! })
//! ```
//!
//! Grammar shape (confirmed via live AST probe against tree-sitter-go):
//! - Node kind: `call_expression`
//! - Callee: `function` **field** → `identifier` node (text = `"Describe"`, `"It"`, …)
//! - Description string: `arguments` **field** → `argument_list` → first named child
//!   that is an `interpreted_string_literal` → decoded via `base.decode_string_literal`.
//! - Lifecycle calls (`BeforeEach`, `AfterEach`, `BeforeSuite`, `AfterSuite`,
//!   `JustBeforeEach`, `JustAfterEach`) take only a closure argument — no description
//!   string. The callee name is used as the symbol name.

use crate::base::Symbol;
use crate::go::GoExtractor;
use std::path::PathBuf;

fn symbols(code: &str) -> Vec<Symbol> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .expect("load Go grammar");
    let tree = parser.parse(code, None).expect("parse Go");
    let mut ext = GoExtractor::new(
        "go".to_string(),
        "math_test.go".to_string(),
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

// ── Wave-3 tests ─────────────────────────────────────────────────────────

#[test]
fn ginkgo_describe_context_it_lifecycle_emit_test_role_metadata() {
    let code = r#"package math_test

var _ = Describe("math module", func() {
    Context("addition", func() {
        BeforeEach(func() {})
        AfterEach(func() {})
        It("should add two numbers", func() {
            Expect(1 + 1).To(Equal(2))
        })
        Specify("one plus one equals two", func() {
            Expect(1 + 1).To(Equal(2))
        })
    })
    BeforeSuite(func() {})
    AfterSuite(func() {})
})
"#;
    let syms = symbols(code);

    let desc = syms
        .iter()
        .find(|s| s.name == "math module")
        .unwrap_or_else(|| panic!("expected Describe container, got: {syms:?}"));
    assert!(meta_bool(desc, "test_container"), "Describe → test_container");
    assert!(!meta_bool(desc, "is_test"), "container is not a test case");

    let ctx = syms
        .iter()
        .find(|s| s.name == "addition")
        .unwrap_or_else(|| panic!("expected Context container, got: {syms:?}"));
    assert!(meta_bool(ctx, "test_container"), "Context → test_container");

    let it = syms
        .iter()
        .find(|s| s.name == "should add two numbers")
        .unwrap_or_else(|| panic!("expected It test case, got: {syms:?}"));
    assert!(meta_bool(it, "is_test"), "It → is_test");
    assert!(!meta_bool(it, "test_container"), "test is not a container");

    let specify = syms
        .iter()
        .find(|s| s.name == "one plus one equals two")
        .unwrap_or_else(|| panic!("expected Specify test case, got: {syms:?}"));
    assert!(meta_bool(specify, "is_test"), "Specify → is_test");

    for lifecycle_name in ["BeforeEach", "AfterEach", "BeforeSuite", "AfterSuite"] {
        let lc = syms
            .iter()
            .find(|s| s.name == lifecycle_name)
            .unwrap_or_else(|| panic!("expected {lifecycle_name} lifecycle symbol, got: {syms:?}"));
        assert!(
            meta_bool(lc, "is_test"),
            "{lifecycle_name} → is_test (lifecycle)",
        );
        assert!(
            meta_bool(lc, "test_lifecycle"),
            "{lifecycle_name} → test_lifecycle",
        );
    }
}

#[test]
fn non_ginkgo_go_calls_do_not_become_test_symbols() {
    // fmt.Println, http.Get, and ordinary function calls must not carry test-role
    // metadata.
    let code = r#"package main

import "fmt"

func main() {
    fmt.Println("hello world")
    result := someFunc("argument")
    _ = result
}
"#;
    let syms = symbols(code);
    assert_eq!(
        syms.iter()
            .filter(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container"))
            .count(),
        0,
        "non-Ginkgo calls must not carry test-role metadata: {syms:?}"
    );
}

#[test]
fn selector_calls_with_vocab_names_are_not_test_symbols() {
    // Locks in the `function_node.kind() != "identifier"` selector guard.
    // `req.Context()`, `s.It("…")`, `s.Describe("…", func(){})` all have
    // TRAILING names that are in the Ginkgo vocab — but they are
    // `selector_expression` callees, not bare identifiers. Without the guard
    // they would silently materialise as test containers/cases across ordinary
    // Go web and struct code. If the guard is ever removed, this test fails.
    let code = r#"package main

import "net/http"

func handler(req *http.Request) {
    ctx := req.Context()
    _ = ctx
}

type suite struct{}

func (s suite) run() {
    s.It("not a ginkgo test")
    s.Describe("nope", func() {})
    s.BeforeEach(func() {})
}
"#;
    let syms = symbols(code);
    assert_eq!(
        syms.iter()
            .filter(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container"))
            .count(),
        0,
        "selector calls with vocab names must NOT produce test-role metadata: {syms:?}"
    );
}
