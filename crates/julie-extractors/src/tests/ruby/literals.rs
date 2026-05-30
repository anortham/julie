//! Ruby string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Like the Python reference leg, the extractor captures string literals passed
//! to `call` nodes **config-free**: the `carrier` is the verbatim callee text
//! (bare `method`, or the `receiver.method` join) and `kind` is always `Other`.
//! URL/SQL classification and the carrier gate happen later in the `src/`
//! pipeline. These tests assert the raw capture: text decoding (incl. `#{}`
//! interpolation holes), carrier derivation (bare `execute`, dotted
//! `Net::HTTP.get`/`conn.execute`), `arg_position` over the full list,
//! keyword/hash-argument descent, and enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::ruby::RubyExtractor;
use std::path::PathBuf;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_ruby::LANGUAGE.into())
        .expect("load Ruby grammar");
    let tree = parser.parse(code, None).expect("parse Ruby");
    let mut ext = RubyExtractor::new(
        "test.rb".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.base().literals.clone()
}

#[test]
fn net_http_get_string_arg_captured_with_dotted_carrier() {
    // `Net::HTTP.get("https://api/users")` — member callee, so the carrier is the
    // `receiver.method` join `Net::HTTP.get`. kind stays Other (the gate is a
    // later src/ pass); the literal anchors to the enclosing method.
    let code = r#"
def load
  Net::HTTP.get("https://api/users")
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/users")
        .unwrap_or_else(|| panic!("expected one literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("Net::HTTP.get"),
        "dotted callee carrier is receiver.method"
    );
    assert_eq!(lit.arg_position, 0, "first argument");
    assert_eq!(
        lit.kind,
        LiteralKind::Other,
        "extractor emits Other; carrier classification is a src/ pass"
    );
    assert!(
        lit.containing_symbol_id.is_some(),
        "literal anchored to the enclosing method symbol"
    );
}

#[test]
fn local_receiver_execute_sql_arg_captured() {
    // Local-receiver DB call: `conn.execute("SELECT ... FROM users")`. The carrier
    // `conn.execute` is captured verbatim; the gate later matches the bare
    // `execute` config by last segment.
    let code = r#"
def fetch(conn)
  conn.execute("SELECT id, name FROM users WHERE id = 1")
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("FROM users"))
        .unwrap_or_else(|| panic!("expected the sql literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("conn.execute"));
}

#[test]
fn interpolated_string_decodes_substitution_holes() {
    // `#{id}` interpolation is decoded to a `{}` placeholder so the resolver sees
    // the static SQL shape.
    let code = r#"
def fetch(conn, id)
  conn.execute("SELECT * FROM t WHERE id = #{id}")
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.starts_with("SELECT * FROM t"))
        .unwrap_or_else(|| panic!("expected the sql literal, got {literals:?}"));
    assert_eq!(
        lit.literal_text, "SELECT * FROM t WHERE id = {}",
        "interpolation hole replaced by {{}}"
    );
    assert_eq!(lit.carrier.as_deref(), Some("conn.execute"));
}

#[test]
fn bare_identifier_callee_yields_name_carrier() {
    // `execute("SELECT 1")` — a receiverless call gives the bare method name.
    let code = r#"
def run
  execute("SELECT 1")
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT 1")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("execute"));
}

#[test]
fn keyword_hash_argument_value_is_captured() {
    // `client.get(url: "/api/health")` — the string lives inside a `pair`; the arm
    // descends to its `value` so it is still captured.
    let code = r#"
def load(client)
  client.get(url: "/api/health")
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "/api/health")
        .unwrap_or_else(|| panic!("expected the keyword-arg literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("client.get"));
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `client.request(42, "/api/x")` — the string is the SECOND argument, so
    // arg_position is counted over ALL args and must be 1, not 0.
    let code = r#"
def load(client)
  client.request(42, "/api/x")
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
}
