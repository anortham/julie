//! Lua busted call-style test detection (Miller bridge test-roles).
//!
//! busted declares tests as call expressions (`describe(...)`, `it(...)`,
//! `before_each(...)`), not named function declarations. The lua extractor
//! recognizes these via the shared `crate::test_calls` core and emits the
//! canonical `is_test` / `test_container` / `test_lifecycle` metadata, byte-
//! identical to the JS/TS and Dart call-style paths. These tests assert that
//! metadata on the public `extract_symbols` output and confirm that non-DSL
//! calls (`print`, `assert.equal`) do NOT become test symbols.

use crate::base::Symbol;
use crate::lua::LuaExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn symbols(code: &str) -> Vec<Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .expect("load Lua grammar");
    let tree = parser.parse(code, None).expect("parse Lua");
    let mut ext = LuaExtractor::new(
        "lua".to_string(),
        "spec.lua".to_string(),
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
fn lua_qualified_callee_is_not_materialized() {
    // FALSE-POSITIVE GUARD (#66): `classify_call` keys on the segment before the
    // first '.', so a dot-index method call whose RECEIVER is a vocab word
    // (`it.register("x")`, a `dot_index_expression`) would otherwise be
    // misclassified as a busted `it`. Only a bare-identifier callee is a DSL call.
    let code = r#"
it.register("plugin", function() end)
"#;
    let syms = symbols(code);
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "qualified callee `it.register(...)` must not materialize a test symbol, got {syms:?}"
    );
}

#[test]
fn busted_it_describe_before_each_emit_test_role_metadata() {
    let code = r#"
describe("math helpers", function()
  before_each(function() end)
  it("adds two numbers", function()
    assert.equal(2, 1 + 1)
  end)
end)
"#;
    let syms = symbols(code);

    let it = syms
        .iter()
        .find(|s| s.name == "adds two numbers")
        .unwrap_or_else(|| panic!("expected an `it` test symbol, got {syms:?}"));
    assert!(meta_bool(it, "is_test"), "it() is a test case");

    let describe = syms
        .iter()
        .find(|s| s.name == "math helpers")
        .unwrap_or_else(|| panic!("expected a `describe` container symbol, got {syms:?}"));
    assert!(
        meta_bool(describe, "test_container"),
        "describe() is a test container"
    );
    assert!(
        !meta_bool(describe, "is_test"),
        "a container is not itself a test case"
    );

    let before = syms
        .iter()
        .find(|s| s.name == "before_each")
        .unwrap_or_else(|| panic!("expected a `before_each` lifecycle symbol, got {syms:?}"));
    assert!(
        meta_bool(before, "is_test"),
        "a lifecycle hook counts as a test for is_test"
    );
    assert!(
        meta_bool(before, "test_lifecycle"),
        "before_each is a lifecycle hook"
    );
}

#[test]
fn non_dsl_calls_do_not_become_test_symbols() {
    // `print(...)` and `assert.equal(...)` (a method call) are not busted DSL —
    // their string args must not be materialized as test symbols.
    let code = r#"
local function helper()
  print("not a test")
  assert.equal(1, 1)
end
"#;
    let syms = symbols(code);
    assert!(
        syms.iter().all(|s| s.name != "not a test"),
        "string args of non-DSL calls must not become symbols: {syms:?}"
    );
    assert_eq!(
        syms.iter().filter(|s| meta_bool(s, "is_test")).count(),
        0,
        "no is_test metadata should come from non-DSL calls: {syms:?}"
    );
}
