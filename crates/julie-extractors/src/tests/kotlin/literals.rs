//! Kotlin string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding (incl. `${x}` template holes → `{}`),
//! carrier derivation (bare + dotted `db.execute`), named-argument value
//! descent, `arg_position`, and enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::kotlin::KotlinExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
        .expect("load Kotlin grammar");
    let tree = parser.parse(code, None).expect("parse Kotlin");
    let mut ext = KotlinExtractor::new(
        "kotlin".to_string(),
        "test.kt".to_string(),
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
fun load(): String {
    return greet("hello")
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
fn template_string_named_arg_decodes_interpolation_holes() {
    // `fetch(url = "https://api/${id}/orders")` — a named argument whose value is
    // a Kotlin string template. The `${id}` hole decodes to `{}`, and the value
    // descent past the `url =` name must still capture it.
    let code = r#"
fun load(id: String) {
    fetch(url = "https://api/${id}/orders")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/{}/orders")
        .unwrap_or_else(|| panic!("expected interpolation decoded to {{}}, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("fetch"), "bare callee carrier");
    assert_eq!(lit.arg_position, 0, "named arg is still the first argument");
}

#[test]
fn dotted_member_callee_yields_receiver_member_carrier() {
    // `db.execute("DELETE FROM users")` — the callee is a navigation_expression;
    // the carrier must be the `receiver.member` join so config can match the
    // bare DB verb `execute` via the last-segment rule.
    let code = r#"
fun load() {
    db.execute("DELETE FROM users")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "DELETE FROM users")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("db.execute"),
        "member-call carrier is receiver.member"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `request("GET", "https://api.example.com")` — the URL string is the SECOND
    // argument; arg_position is counted over ALL arguments, so it must be 1.
    let code = r#"
fun load() {
    request("GET", "https://api.example.com")
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
fun load() {
    log("first", "second")
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
