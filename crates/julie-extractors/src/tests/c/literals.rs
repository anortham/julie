//! C string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding, carrier derivation (bare +
//! `recv.field` for a function-pointer member call), `arg_position` over the full
//! argument list (so the SQL in `sqlite3_exec(db, "...")` and the URL in
//! `curl_easy_setopt(h, CURLOPT_URL, "...")` report the right ordinal), and
//! enclosing-symbol anchoring. C has no string interpolation.

use crate::base::{Literal, LiteralKind};
use crate::c::CExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .expect("load C grammar");
    let tree = parser.parse(code, None).expect("parse C");
    let mut ext = CExtractor::new(
        "c".to_string(),
        "test.c".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_literals()
}

#[test]
fn bare_function_call_arg_captured_with_carrier() {
    // `greet("hello")` — one string-literal arg with a plain-identifier callee.
    // Recorded verbatim with carrier="greet", arg_position=0, kind=Other, and
    // anchored to the enclosing function.
    let code = r#"
const char *load(void) {
    return greet("hello");
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
fn sqlite_exec_sql_arg_reports_full_arg_position() {
    // `sqlite3_exec(db, "SELECT * FROM users", 0, 0, 0)` — the SQL is the SECOND
    // argument; arg_position is counted over ALL arguments, so it must be 1.
    let code = r#"
void load(sqlite3 *db) {
    sqlite3_exec(db, "SELECT * FROM users", 0, 0, 0);
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT * FROM users")
        .unwrap_or_else(|| panic!("expected the SQL literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("sqlite3_exec"),
        "bare callee carrier"
    );
    assert_eq!(lit.arg_position, 1, "SQL string is the second argument");
}

#[test]
fn curl_setopt_url_reports_third_arg_position() {
    // `curl_easy_setopt(h, CURLOPT_URL, "https://...")` — the URL is the THIRD
    // argument, so arg_position must be 2.
    let code = r#"
void load(CURL *h) {
    curl_easy_setopt(h, CURLOPT_URL, "https://api.example.com/users");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api.example.com/users")
        .unwrap_or_else(|| panic!("expected the URL literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("curl_easy_setopt"),
        "bare callee carrier"
    );
    assert_eq!(lit.arg_position, 2, "URL string is the third argument");
}

#[test]
fn function_pointer_member_callee_yields_object_field_carrier() {
    // `state->log_fn("connecting")` — the callee is a field_expression (call
    // through a function-pointer member); the carrier must be the `object.field`
    // join so the gate's last-segment rule can match a bare `log_fn` config.
    let code = r#"
void load(struct conn *state) {
    state->log_fn("connecting");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "connecting")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("state.log_fn"),
        "function-pointer member carrier is object.field"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn multiple_string_args_each_captured_carrier_agnostic() {
    // `printf("first", "second")` — the extractor is carrier-AGNOSTIC: it
    // captures BOTH string args (carrier printf, positions 0 and 1). Dropping
    // non-carrier literals is the src/ pipeline's job, not the extractor's.
    let code = r#"
void load(void) {
    printf("first", "second");
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
            Some("printf"),
            "carrier is the callee for every arg"
        );
    }
}
