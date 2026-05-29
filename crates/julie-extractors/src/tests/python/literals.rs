//! Python string-literal call-argument capture (Miller bridge Phase 3).
//!
//! Like the TS/C# reference legs, the extractor captures string literals passed
//! to calls **config-free**: the `carrier` is the verbatim callee text and
//! `kind` is always `Other`. URL/SQL classification and the carrier gate happen
//! later in the `src/` pipeline. These tests assert the raw capture: text
//! decoding (incl. f-string interpolation holes), carrier derivation (bare
//! `open`, dotted `requests.get`/`cursor.execute`), `arg_position` over the full
//! list, keyword-argument descent, and enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::python::PythonExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("load Python grammar");
    let tree = parser.parse(code, None).expect("parse Python");
    let mut ext = PythonExtractor::new(
        "test.py".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.base().literals.clone()
}

#[test]
fn requests_get_string_arg_captured_with_dotted_carrier() {
    // `requests.get("https://api/users")` — member callee, so the carrier is the
    // `object.attribute` join `requests.get`. kind stays Other (the gate is a
    // later src/ pass); the literal anchors to the enclosing function.
    let code = r#"
def load():
    return requests.get("https://api/users")
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/users")
        .unwrap_or_else(|| panic!("expected one literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("requests.get"),
        "dotted callee carrier is object.attribute"
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
fn cursor_execute_sql_arg_captured() {
    // Local-receiver DB call: `cursor.execute("SELECT ... FROM users")`. The
    // carrier `cursor.execute` is captured verbatim; the gate later matches the
    // bare `execute` config by last segment.
    let code = r#"
def fetch(cursor):
    cursor.execute("SELECT id, name FROM users WHERE id = %s")
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("FROM users"))
        .unwrap_or_else(|| panic!("expected the sql literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("cursor.execute"));
}

#[test]
fn fstring_arg_decodes_interpolation_holes() {
    // f-string `f"/api/users/{uid}/orders"` — each interpolation is decoded to a
    // `{}` placeholder so the resolver sees the static URL shape.
    let code = r#"
def load(uid):
    return requests.get(f"/api/users/{uid}/orders")
"#;
    let literals = capture(code);
    assert_eq!(
        literals.len(),
        1,
        "one literal for the f-string arg, got {literals:?}"
    );
    assert_eq!(
        literals[0].literal_text, "/api/users/{}/orders",
        "interpolation hole replaced by {{}}"
    );
    assert_eq!(literals[0].carrier.as_deref(), Some("requests.get"));
}

#[test]
fn bare_identifier_callee_yields_name_carrier() {
    // `open("/etc/hosts")` — a plain identifier callee gives the bare name.
    let code = r#"
def load():
    return open("/etc/hosts")
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "/etc/hosts")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("open"));
}

#[test]
fn keyword_argument_value_is_captured() {
    // `requests.get(url="/api/health")` — the string lives inside a
    // keyword_argument; the arm descends to its `value` so it is still captured.
    let code = r#"
def load():
    return requests.get(url="/api/health")
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "/api/health")
        .unwrap_or_else(|| panic!("expected the keyword-arg literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("requests.get"));
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `client.request(42, "/api/x")` — the string is the SECOND argument, so
    // arg_position is counted over ALL args and must be 1, not 0.
    let code = r#"
def load(client):
    return client.request(42, "/api/x")
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
}
