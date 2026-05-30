//! Swift string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding, carrier derivation (bare identifier +
//! dotted `db.execute`), labeled-argument `value` descent, `arg_position`, and
//! enclosing-symbol anchoring.
//!
//! Swift interpolation (`\(x)`) parses as an `interpolated_expression` named
//! child; the shared `decode_string_literal` normalizes it to a `{}` placeholder
//! (see `interpolation_hole_is_normalized_to_placeholder`).

use crate::base::{Literal, LiteralKind};
use crate::swift::SwiftExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .expect("load Swift grammar");
    let tree = parser.parse(code, None).expect("parse Swift");
    let mut ext = SwiftExtractor::new(
        "swift".to_string(),
        "test.swift".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_literals()
}

#[test]
fn bare_function_call_arg_captured_with_carrier() {
    // `greet("hello")` — plain simple_identifier callee. Recorded verbatim with
    // carrier="greet", arg_position=0, kind=Other, anchored to the enclosing fn.
    let code = r#"
func load() -> String {
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
fn dotted_member_callee_yields_target_suffix_carrier() {
    // `db.execute("DELETE FROM users")` — the callee is a navigation_expression;
    // the carrier must be the `target.suffix` join so config can match the bare
    // DB verb `execute` via the last-segment rule.
    let code = r#"
func load() {
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
        "member-call carrier is target.suffix"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn labeled_argument_value_is_captured() {
    // `request(url: "https://api.example.com")` — the string is the `value` of a
    // labeled `value_argument`; the descent into `value` must still capture it.
    let code = r#"
func load() {
    request(url: "https://api.example.com")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api.example.com")
        .unwrap_or_else(|| panic!("expected a labeled-arg literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("request"),
        "bare callee carrier"
    );
    assert_eq!(
        lit.arg_position, 0,
        "labeled arg is still the first argument"
    );
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `request("GET", "https://api.example.com")` — the URL string is the SECOND
    // argument; arg_position is counted over ALL arguments, so it must be 1.
    let code = r#"
func load() {
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
func load() {
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

#[test]
fn interpolation_hole_is_normalized_to_placeholder() {
    // `"users/\(id)/profile"` — Swift interpolation parses as an
    // `interpolated_expression` named child (the `\(` `)` markers are anonymous
    // siblings, the surrounding text is `line_str_text`). The shared decoder must
    // replace the hole with `{}` so a resolver sees the static route shape
    // (`users/{}/profile`) instead of silently dropping the hole.
    let code = r#"
func load(id: String) {
    request(url: "users/\(id)/profile")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("users/"))
        .unwrap_or_else(|| panic!("expected the interpolated URL literal, got {literals:?}"));
    assert_eq!(
        lit.literal_text, "users/{}/profile",
        "interpolation hole must normalize to a {{}} placeholder"
    );
}
