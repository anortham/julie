//! Scala string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding (plain Scala `string` nodes have no
//! content child, so they exercise the delimiter-strip fallback), carrier
//! derivation (bare apply + dotted `requests.get`), `arg_position`, and
//! enclosing-symbol anchoring. Prefixed interpolators (Doobie `sql"..."`,
//! sttp `uri"..."`) are not call-argument literals and are out of scope.

use crate::base::{Literal, LiteralKind};
use crate::scala::ScalaExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_scala::LANGUAGE.into())
        .expect("load Scala grammar");
    let tree = parser.parse(code, None).expect("parse Scala");
    let mut ext = ScalaExtractor::new(
        "scala".to_string(),
        "test.scala".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_literals()
}

#[test]
fn bare_function_call_arg_captured_with_carrier() {
    // `greet("hello")` — plain-identifier callee. Recorded verbatim with
    // carrier="greet", arg_position=0, kind=Other, anchored to the enclosing fn.
    let code = r#"
object Loader {
  def load(): String = greet("hello")
}
"#;
    let literals = capture(code);
    let hits: Vec<&Literal> = literals
        .iter()
        .filter(|l| l.literal_text == "hello")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "exactly one literal for the arg, got {literals:?}"
    );
    let lit = hits[0];
    assert_eq!(lit.carrier.as_deref(), Some("greet"), "bare callee carrier");
    assert_eq!(lit.arg_position, 0, "first argument");
    assert_eq!(
        lit.kind,
        LiteralKind::Other,
        "extractor emits Other; carrier classification is a src/ pass"
    );
    assert!(
        lit.containing_symbol_id.is_some(),
        "literal anchored to the enclosing symbol"
    );
}

#[test]
fn bare_apply_captures_sql_via_delimiter_fallback() {
    // `SQL("SELECT * FROM users")` (Anorm) — a plain Scala `string` exposes no
    // content child, so decode_string_literal falls back to stripping the outer
    // quotes. The SQL must come through verbatim with carrier="SQL".
    let code = r#"
object Repo {
  def all() = SQL("SELECT * FROM users")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT * FROM users")
        .unwrap_or_else(|| panic!("expected the SQL literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("SQL"), "bare apply carrier");
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn dotted_member_callee_yields_value_field_carrier() {
    // `requests.get("https://api.example.com")` — the callee is a
    // field_expression; the carrier must be the `value.field` join so config can
    // match `requests.get`.
    let code = r#"
object Loader {
  def load() = requests.get("https://api.example.com")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api.example.com")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("requests.get"),
        "member-call carrier is value.field"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `request("GET", "https://api.example.com")` — the URL string is the SECOND
    // argument; arg_position is counted over ALL arguments, so it must be 1.
    let code = r#"
object Loader {
  def load() = request("GET", "https://api.example.com")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api.example.com")
        .unwrap_or_else(|| panic!("expected the URL literal, got {literals:?}"));
    assert_eq!(
        lit.arg_position, 1,
        "string at second position must report arg_position 1"
    );
}

#[test]
fn multiple_string_args_each_captured_carrier_agnostic() {
    // `log("first", "second")` — the extractor is carrier-AGNOSTIC: it captures
    // BOTH string args (carrier "log", positions 0 and 1). Dropping non-carrier
    // literals is the src/ pipeline's job, not the extractor's.
    let code = r#"
object Loader {
  def load() = log("first", "second")
}
"#;
    let literals = capture(code);
    let texts: Vec<&str> = literals.iter().map(|l| l.literal_text.as_str()).collect();
    assert!(
        texts.contains(&"first") && texts.contains(&"second"),
        "both string args captured at the extractor layer, got {texts:?}"
    );
    for l in &literals {
        assert_eq!(
            l.carrier.as_deref(),
            Some("log"),
            "carrier is the bare callee for every arg"
        );
    }
}
