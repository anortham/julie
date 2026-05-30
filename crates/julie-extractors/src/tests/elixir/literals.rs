//! Elixir string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Like the Python reference leg, the extractor captures string literals passed
//! to `call` nodes **config-free**: the `carrier` is the verbatim callee (bare
//! function name, or the `Module.function` join for a `dot` target) and `kind`
//! is always `Other`. URL/SQL classification and the carrier gate happen later
//! in the `src/` pipeline. These tests assert the raw capture: text decoding
//! (incl. `#{}` interpolation holes), carrier derivation (bare `execute`,
//! dotted `HTTPoison.get`/`Repo.query`), `arg_position` over the full list, and
//! enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::elixir::ElixirExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_elixir::LANGUAGE.into())
        .expect("load Elixir grammar");
    let tree = parser.parse(code, None).expect("parse Elixir");
    let mut ext = ElixirExtractor::new(
        "elixir".to_string(),
        "test.ex".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.base.literals.clone()
}

#[test]
fn httpoison_get_string_arg_captured_with_dotted_carrier() {
    // `HTTPoison.get("https://api/users")` — `dot` target, so the carrier is the
    // `Module.function` join `HTTPoison.get`. kind stays Other; the literal
    // anchors to the enclosing function.
    let code = r#"
defmodule M do
  def load do
    HTTPoison.get("https://api/users")
  end
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/users")
        .unwrap_or_else(|| panic!("expected one literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("HTTPoison.get"),
        "dotted callee carrier is Module.function"
    );
    assert_eq!(lit.arg_position, 0, "first argument");
    assert_eq!(
        lit.kind,
        LiteralKind::Other,
        "extractor emits Other; carrier classification is a src/ pass"
    );
    assert!(
        lit.containing_symbol_id.is_some(),
        "literal anchored to the enclosing function symbol"
    );
}

#[test]
fn repo_query_sql_arg_captured() {
    // `Repo.query("SELECT ... FROM users")`. The carrier `Repo.query` is captured
    // verbatim; the gate later matches the bare `query` config by last segment.
    let code = r#"
defmodule M do
  def fetch do
    Repo.query("SELECT id, name FROM users")
  end
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("FROM users"))
        .unwrap_or_else(|| panic!("expected the sql literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("Repo.query"));
}

#[test]
fn interpolated_string_decodes_substitution_holes() {
    // `#{uid}` interpolation is decoded to a `{}` placeholder so the resolver
    // sees the static URL shape.
    let code = r#"
defmodule M do
  def load(uid) do
    Req.get("/api/users/#{uid}/orders")
  end
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.starts_with("/api/users/"))
        .unwrap_or_else(|| panic!("expected the url literal, got {literals:?}"));
    assert_eq!(
        lit.literal_text, "/api/users/{}/orders",
        "interpolation hole replaced by {{}}"
    );
    assert_eq!(lit.carrier.as_deref(), Some("Req.get"));
}

#[test]
fn bare_identifier_callee_yields_name_carrier() {
    // `execute("CREATE TABLE t")` — a bare `identifier` target gives the bare name.
    let code = r#"
defmodule M do
  def up do
    execute("CREATE TABLE t (id integer)")
  end
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.starts_with("CREATE TABLE"))
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("execute"));
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `Tesla.get(client, "/api/x")` — the string is the SECOND argument, so
    // arg_position is counted over ALL args and must be 1, not 0.
    let code = r#"
defmodule M do
  def load(client) do
    Tesla.get(client, "/api/x")
  end
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "/api/x")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.arg_position, 1,
        "string at second position must report arg_position 1"
    );
    assert_eq!(lit.carrier.as_deref(), Some("Tesla.get"));
}
