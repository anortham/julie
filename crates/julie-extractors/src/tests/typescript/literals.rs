//! TypeScript string-literal call-argument capture (Miller bridge Phase 3).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding, carrier derivation (incl. dotted
//! `axios.get`), `arg_position`, and enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::typescript::TypeScriptExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("load TypeScript grammar");
    let tree = parser.parse(code, None).expect("parse TypeScript");
    let mut ext = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.base().literals.clone()
}

#[test]
fn fetch_string_arg_captured_as_literal_with_carrier() {
    // `fetch("/api/users")` — one string-literal arg. The extractor records it
    // verbatim with carrier="fetch", arg_position=0, kind=Other (classification
    // to Url is a later src/ pass). It must be anchored to the enclosing fn.
    let code = r#"
function load() {
    return fetch("/api/users");
}
"#;
    let literals = capture(code);
    let urls: Vec<&Literal> = literals
        .iter()
        .filter(|l| l.literal_text == "/api/users")
        .collect();
    assert_eq!(
        urls.len(),
        1,
        "exactly one literal for the fetch arg, got {literals:?}"
    );
    let lit = urls[0];
    assert_eq!(
        lit.carrier.as_deref(),
        Some("fetch"),
        "carrier is the callee"
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
fn template_string_arg_decodes_interpolation_holes() {
    // `fetch(`/api/users/${id}`)` — the template substitution `${id}` is decoded
    // to a `{}` placeholder so the resolver sees the static URL shape.
    let code = r#"
function load(id: string) {
    return fetch(`/api/users/${id}/orders`);
}
"#;
    let literals = capture(code);
    assert_eq!(
        literals.len(),
        1,
        "one literal for the template arg, got {literals:?}"
    );
    assert_eq!(
        literals[0].literal_text, "/api/users/{}/orders",
        "interpolation hole replaced by {{}}"
    );
    assert_eq!(literals[0].carrier.as_deref(), Some("fetch"));
}

#[test]
fn dotted_member_callee_yields_object_property_carrier() {
    // `axios.get("/api/users")` — the `function` is a member_expression; the
    // carrier must be the `object.property` join so config can match `axios.get`.
    let code = r#"
function load() {
    return axios.get("/api/users");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "/api/users")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("axios.get"),
        "dotted callee carrier is object.property"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `request(42, "/api/x")` — the string is the SECOND argument; arg_position
    // is counted over ALL arguments, so it must be 1, not 0.
    let code = r#"
function load() {
    return request(42, "/api/x");
}
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

#[test]
fn multiple_string_args_each_captured_carrier_agnostic() {
    // `console.log("a", "b")` — the extractor is carrier-AGNOSTIC: it captures
    // BOTH string args (carrier console.log, positions 0 and 1). Dropping
    // non-carrier literals is the src/ pipeline's job, not the extractor's.
    let code = r#"
function load() {
    console.log("first", "second");
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
            Some("console.log"),
            "carrier is the dotted callee for every arg"
        );
    }
}
