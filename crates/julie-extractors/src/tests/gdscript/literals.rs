//! GDScript string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Like the Python reference leg, the extractor captures string literals passed
//! to calls **config-free**: `kind` is always `Other` and the `carrier` is the
//! verbatim callee. `recv.method(args)` parses as `attribute { recv,
//! attribute_call }`, so the call args live on the `attribute_call` node — the
//! carrier is the `receiver.method` join (`http.request`). A bare `call`
//! (`load("res://…")`) yields the plain identifier carrier. URL/SQL
//! classification and the carrier gate happen later in the `src/` pipeline.
//! GDScript string literals have no interpolation, so decoding is a plain
//! delimiter strip. These tests assert raw capture: carrier derivation (bare AND
//! dotted), `arg_position` over the full list, and enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::gdscript::GDScriptExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .expect("load GDScript grammar");
    let tree = parser.parse(code, None).expect("parse GDScript");
    let mut ext = GDScriptExtractor::new(
        "gdscript".to_string(),
        "test.gd".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.base().literals.clone()
}

#[test]
fn http_request_attribute_call_captured_with_dotted_carrier() {
    // `http.request("https://api/users")` — an `attribute_call`, so the carrier is
    // the `receiver.method` join `http.request`. kind stays Other; the literal
    // anchors to the enclosing function.
    let code = "func load():\n\thttp.request(\"https://api/users\")\n";
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/users")
        .unwrap_or_else(|| panic!("expected one literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("http.request"),
        "attribute_call carrier is receiver.method"
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
fn db_query_sql_arg_captured() {
    // `db.query("SELECT ... FROM users")`. The carrier `db.query` is captured
    // verbatim; the gate later matches the bare `query` config by last segment.
    let code = "func fetch():\n\tdb.query(\"SELECT id, name FROM users\")\n";
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("FROM users"))
        .unwrap_or_else(|| panic!("expected the sql literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("db.query"));
}

#[test]
fn bare_call_yields_name_carrier() {
    // `load("res://scenes/main.tscn")` — a bare `call` with a plain identifier
    // callee gives the bare name.
    let code = "func ready():\n\tload(\"res://scenes/main.tscn\")\n";
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "res://scenes/main.tscn")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("load"));
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `client.request(42, "/api/x")` — the string is the SECOND argument, so
    // arg_position is counted over ALL args and must be 1, not 0.
    let code = "func load():\n\tclient.request(42, \"/api/x\")\n";
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "/api/x")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.arg_position, 1,
        "string at second position must report arg_position 1"
    );
    assert_eq!(lit.carrier.as_deref(), Some("client.request"));
}
