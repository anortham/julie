//! Rust string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding, carrier derivation (bare identifier,
//! `value.field` method call, `qualifier.name` scoped path), `arg_position`,
//! and enclosing-symbol anchoring. sqlx `query!`/`query_as!`/`query_scalar!`
//! macros are `macro_invocation` nodes (not `call_expression`); they are
//! captured by the dedicated macro arm and covered by the two macro tests at
//! the end of this file (carrier = the macro name's last segment).

use crate::base::{Literal, LiteralKind};
use crate::rust::RustExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("load Rust grammar");
    let tree = parser.parse(code, None).expect("parse Rust");
    let mut ext = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
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
fn load() -> String {
    greet("hello")
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
        "literal anchored to the enclosing function symbol"
    );
}

#[test]
fn method_call_callee_yields_value_field_carrier() {
    // `conn.execute("INSERT INTO users VALUES (1)")` — the callee is a
    // field_expression; the carrier must be the `value.field` join so config
    // can match the bare DB verb `execute` via the last-segment rule.
    let code = r#"
fn load() {
    conn.execute("INSERT INTO users VALUES (1)");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "INSERT INTO users VALUES (1)")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("conn.execute"),
        "method-call carrier is value.field"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn scoped_path_callee_yields_qualifier_name_carrier() {
    // `reqwest::get("https://api.example.com")` — the callee is a
    // scoped_identifier; the carrier must be the immediate `qualifier.name`
    // join so config can match `reqwest.get`.
    let code = r#"
fn load() {
    reqwest::get("https://api.example.com");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api.example.com")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("reqwest.get"),
        "scoped-path carrier is qualifier.name"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `request(42, "/api/x")` — the string is the SECOND argument; arg_position
    // is counted over ALL arguments, so it must be 1, not 0.
    let code = r#"
fn load() {
    request(42, "/api/x");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "/api/x")
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
fn load() {
    log("first", "second");
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

#[test]
fn sqlx_query_macro_captures_sql_with_last_segment_carrier() {
    // `sqlx::query!("SELECT ...", id)` — the dominant Rust SQL form is a
    // macro_invocation, not a call_expression. The SQL string lives inside the
    // macro's token-tree; it must be captured with carrier = the macro name's
    // last segment ("query", no `!`), kind=Other, anchored to the enclosing fn.
    // The SQL is the first macro arg, so arg_position is 0.
    let code = r#"
async fn load(id: i64) {
    sqlx::query!("SELECT id FROM users WHERE id = $1", id);
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("FROM users"))
        .unwrap_or_else(|| panic!("expected the sqlx macro SQL literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("query"),
        "macro carrier is the macro name's last segment, no `!`"
    );
    assert_eq!(
        lit.literal_text, "SELECT id FROM users WHERE id = $1",
        "the full SQL string decodes verbatim ($1 is literal text, not interpolation)"
    );
    assert_eq!(lit.arg_position, 0, "SQL is the first arg in query!");
    assert_eq!(
        lit.kind,
        LiteralKind::Other,
        "extractor emits Other; carrier classification is a src/ pass"
    );
    assert!(
        lit.containing_symbol_id.is_some(),
        "literal anchored to the enclosing async fn"
    );
}

#[test]
fn query_as_macro_captures_sql_after_type_argument() {
    // `query_as!(User, "SELECT ...")` — query_as! takes a TYPE first, then the
    // SQL string. Capture it wherever it appears in the token-tree; assert on
    // literal_text + carrier rather than a fixed position. Raw-string form too.
    let code = r#"
async fn load() {
    query_as!(User, "SELECT * FROM accounts");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT * FROM accounts")
        .unwrap_or_else(|| panic!("expected the query_as macro SQL literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("query_as"),
        "macro carrier is the macro name, no `!`"
    );
    assert_eq!(lit.kind, LiteralKind::Other);
    assert!(lit.containing_symbol_id.is_some());
}
