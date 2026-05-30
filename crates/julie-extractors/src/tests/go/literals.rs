//! Go string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding (incl. Go raw strings), carrier
//! derivation (bare + dotted `http.Get`), `arg_position`, and enclosing-symbol
//! anchoring. Go has no string interpolation, so raw/interpreted strings decode
//! to their verbatim contents.

use crate::base::{Literal, LiteralKind};
use crate::go::GoExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .expect("load Go grammar");
    let tree = parser.parse(code, None).expect("parse Go");
    let mut ext = GoExtractor::new(
        "go".to_string(),
        "test.go".to_string(),
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
    let code = r#"package main

func load() string {
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
fn raw_string_arg_decodes_to_verbatim_contents() {
    // Backtick raw string `SELECT * FROM users` passed to a dotted DB call.
    // decode_string_literal must strip the backticks and keep the SQL verbatim.
    let code = "package main\n\nfunc load() {\n    db.Query(`SELECT * FROM users`)\n}\n";
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT * FROM users")
        .unwrap_or_else(|| panic!("expected a raw-string literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("db.Query"),
        "dotted callee carrier is operand.field"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn dotted_member_callee_yields_operand_field_carrier() {
    // `http.Get("https://api.example.com/users")` — the callee is a
    // selector_expression; the carrier must be the `operand.field` join so
    // config can match `http.Get`.
    let code = r#"package main

func load() {
    http.Get("https://api.example.com/users")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api.example.com/users")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("http.Get"),
        "dotted callee carrier is operand.field"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `http.NewRequest("GET", "https://api.example.com", nil)` — the URL string
    // is the SECOND argument; arg_position is counted over ALL arguments, so it
    // must be 1, not 0.
    let code = r#"package main

func load() {
    http.NewRequest("GET", "https://api.example.com", nil)
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
    // `fmt.Println("first", "second")` — the extractor is carrier-AGNOSTIC: it
    // captures BOTH string args (carrier fmt.Println, positions 0 and 1).
    // Dropping non-carrier literals is the src/ pipeline's job, not the
    // extractor's.
    let code = r#"package main

func load() {
    fmt.Println("first", "second")
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
            Some("fmt.Println"),
            "carrier is the dotted callee for every arg"
        );
    }
}
