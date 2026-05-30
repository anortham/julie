//! PHP Pest call-style test detection (Miller bridge test-roles).
//!
//! Pest declares tests as call expressions (`test(...)`, `it(...)`,
//! `describe(...)`), not named function declarations. The php extractor
//! recognises these via the shared `crate::test_calls` core and emits the
//! canonical `is_test` / `test_container` / `test_lifecycle` metadata,
//! byte-identical to the Lua/R/JS/TS call-style paths. These tests assert that
//! metadata on the public `extract_symbols` output and confirm that non-DSL
//! calls (`array_map`, `expect(...)` matchers) do NOT become test symbols.

use crate::base::Symbol;
use crate::php::PhpExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn symbols(code: &str) -> Vec<Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .expect("load PHP grammar");
    let tree = parser.parse(code, None).expect("parse PHP");
    let mut ext = PhpExtractor::new(
        "php".to_string(),
        "ExampleTest.php".to_string(),
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
fn php_qualified_callee_is_not_materialized() {
    // FALSE-POSITIVE GUARD (#66): only bare-name `function_call_expression` nodes
    // are Pest DSL calls. Method calls (`$obj->it(...)`, a `member_call_expression`)
    // and static calls (`Klass::describe(...)`, a `scoped_call_expression`) are
    // DIFFERENT node kinds, filtered before classification — even when the METHOD
    // name is a vocab word. Locks in `classify_call_exact` (centralized #66 fix).
    let code = r#"<?php
class Runner {
    public function run(): void {
        $this->it('does work', fn() => null);
        Suite::describe('group', fn() => null);
    }
}
"#;
    let syms = symbols(code);
    assert!(
        !syms
            .iter()
            .any(|s| meta_bool(s, "is_test") || meta_bool(s, "test_container")),
        "method/static callees (`$this->it`, `Suite::describe`) must not materialize a test symbol, got {syms:?}"
    );
}

#[test]
fn pest_test_it_describe_beforeeach_emit_test_role_metadata() {
    let code = r#"<?php

test('computes totals correctly', function () {
    expect(1 + 1)->toBe(2);
});

it('can create a user', function () {
    expect(true)->toBeTrue();
});

describe('User management', function () {
    it('can login', function () {
        expect(true)->toBeTrue();
    });
});

beforeEach(function () {
    // shared setup
});
"#;
    let syms = symbols(code);

    // test('...') → is_test
    let test_sym = syms
        .iter()
        .find(|s| s.name == "computes totals correctly")
        .unwrap_or_else(|| panic!("expected a `test()` symbol, got {syms:?}"));
    assert!(meta_bool(test_sym, "is_test"), "test() should be a test case");
    assert!(
        !meta_bool(test_sym, "test_container"),
        "test() should not be a container"
    );

    // it('...') → is_test
    let it_sym = syms
        .iter()
        .find(|s| s.name == "can create a user")
        .unwrap_or_else(|| panic!("expected an `it()` symbol, got {syms:?}"));
    assert!(meta_bool(it_sym, "is_test"), "it() should be a test case");
    assert!(
        !meta_bool(it_sym, "test_container"),
        "it() should not be a container"
    );

    // describe('...') → test_container (not is_test)
    let describe_sym = syms
        .iter()
        .find(|s| s.name == "User management")
        .unwrap_or_else(|| panic!("expected a `describe()` container symbol, got {syms:?}"));
    assert!(
        meta_bool(describe_sym, "test_container"),
        "describe() should be a test container"
    );
    assert!(
        !meta_bool(describe_sym, "is_test"),
        "a container is not itself a test case"
    );

    // beforeEach(...) → is_test + test_lifecycle
    let before_sym = syms
        .iter()
        .find(|s| s.name == "beforeEach")
        .unwrap_or_else(|| panic!("expected a `beforeEach()` lifecycle symbol, got {syms:?}"));
    assert!(
        meta_bool(before_sym, "is_test"),
        "a lifecycle hook counts as is_test"
    );
    assert!(
        meta_bool(before_sym, "test_lifecycle"),
        "beforeEach should be a lifecycle hook"
    );
}

#[test]
fn non_dsl_calls_do_not_become_test_symbols() {
    // array_map, expect()->toBe() matchers, and bare function definitions are
    // not Pest DSL — their string args must not be materialised as test symbols.
    let code = r#"<?php

$result = array_map(fn($x) => $x * 2, [1, 2, 3]);

function helper(): void {
    echo 'not a test';
}
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
