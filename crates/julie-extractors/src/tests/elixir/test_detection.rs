//! Elixir ExUnit test-role detection signals (Miller bridge test-roles).
//!
//! EXTRACTOR-level assertions; the role classifier lives in the `julie` crate.
//! ExUnit is call/macro-style: `test "…" do`, `describe "…" do`, and the
//! `setup`/`setup_all` lifecycle hooks are all `call` nodes, materialized by the
//! bespoke dispatch in `elixir/calls.rs` (NOT the shared `test_calls` core —
//! `describe` is Namespace-kind, which the shared builder cannot express).
//!
//! Guards the two gaps the call-style breadth ledger (#53) found:
//! - `describe` was a Namespace with `metadata: None` → must carry
//!   `test_container` so the role classifier lights it up.
//! - `setup` / `setup_all` were absent from the dispatch → must materialize as
//!   `is_test` + `test_lifecycle` Function symbols (mirroring JS beforeEach /
//!   Ruby before).

use crate::base::SymbolKind;
use crate::elixir::ElixirExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn symbols(code: &str, file: &str) -> Vec<crate::base::Symbol> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_elixir::LANGUAGE.into())
        .expect("load Elixir grammar");
    let tree = parser.parse(code, None).expect("parse Elixir");
    let mut ext = ElixirExtractor::new(
        "elixir".to_string(),
        file.to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    ext.extract_symbols(&tree)
}

fn meta_bool(symbol: &crate::base::Symbol, key: &str) -> bool {
    symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn describe_block_flagged_test_container() {
    // `describe "addition" do ... end` is materialized as a Namespace symbol for
    // parenting; it must ALSO carry test_container metadata so the role classifier
    // marks it a TestContainer.
    let code = r#"
defmodule CalcTest do
  use ExUnit.Case

  describe "addition" do
    test "adds two numbers" do
      assert 2 + 2 == 4
    end
  end
end
"#;
    let syms = symbols(code, "test/calc_test.exs");
    let desc = syms
        .iter()
        .find(|s| s.name == "addition" && s.kind == SymbolKind::Namespace)
        .unwrap_or_else(|| panic!("expected describe Namespace symbol, got {syms:?}"));
    assert!(
        meta_bool(desc, "test_container"),
        "describe block must be flagged test_container, got {:?}",
        desc.metadata
    );
}

#[test]
fn test_block_remains_is_test_under_describe() {
    // Regression guard: the describe metadata change must not disturb the nested
    // `test "…"` materialization — it stays a Function flagged is_test, parented
    // to the describe block.
    let code = r#"
defmodule CalcTest do
  use ExUnit.Case

  describe "addition" do
    test "adds two numbers" do
      assert 2 + 2 == 4
    end
  end
end
"#;
    let syms = symbols(code, "test/calc_test.exs");
    let desc = syms.iter().find(|s| s.name == "addition").unwrap();
    let t = syms
        .iter()
        .find(|s| s.name == "adds two numbers")
        .unwrap_or_else(|| panic!("expected nested test symbol, got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
    assert!(meta_bool(t, "is_test"), "test block must stay is_test");
    assert_eq!(
        t.parent_id.as_deref(),
        Some(desc.id.as_str()),
        "nested test must parent to the describe block"
    );
}

#[test]
fn setup_hook_materialized_as_lifecycle() {
    // `setup do ... end` — an ExUnit per-test lifecycle hook (call node, no string
    // name). Must materialize a Function named "setup" carrying both is_test and
    // test_lifecycle.
    let code = r#"
defmodule ConnTest do
  use ExUnit.Case

  setup do
    {:ok, conn: build_conn()}
  end
end
"#;
    let syms = symbols(code, "test/conn_test.exs");
    let setup = syms
        .iter()
        .find(|s| s.name == "setup")
        .unwrap_or_else(|| panic!("expected setup lifecycle symbol, got {syms:?}"));
    assert_eq!(setup.kind, SymbolKind::Function);
    assert!(meta_bool(setup, "is_test"), "setup must be is_test");
    assert!(
        meta_bool(setup, "test_lifecycle"),
        "setup must be flagged test_lifecycle"
    );
}

#[test]
fn setup_all_hook_materialized_as_lifecycle() {
    // `setup_all do ... end` — the once-per-module lifecycle hook. Same metadata
    // contract as `setup`.
    let code = r#"
defmodule ConnTest do
  use ExUnit.Case

  setup_all do
    {:ok, started: true}
  end
end
"#;
    let syms = symbols(code, "test/conn_test.exs");
    let setup_all = syms
        .iter()
        .find(|s| s.name == "setup_all")
        .unwrap_or_else(|| panic!("expected setup_all lifecycle symbol, got {syms:?}"));
    assert_eq!(setup_all.kind, SymbolKind::Function);
    assert!(meta_bool(setup_all, "is_test"), "setup_all must be is_test");
    assert!(
        meta_bool(setup_all, "test_lifecycle"),
        "setup_all must be flagged test_lifecycle"
    );
}
