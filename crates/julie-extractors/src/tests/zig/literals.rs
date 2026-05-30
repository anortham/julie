//! Zig string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding, carrier derivation (bare + dotted
//! `std.Uri.parse` / `db.exec`), `arg_position` over the full argument list, and
//! enclosing-symbol anchoring.
//!
//! Zig applicability (NOT N/A): `std.Uri.parse("https://...")` is a real
//! string-literal URL carrier in the std library, and zig-sqlite exposes
//! `db.exec("...")` / `db.prepare("...")` SQL carriers. Zig has no `arguments`
//! wrapper node — the callee is the `function` field and args are the other named
//! children, which the capture skips-by-id and counts over.

use crate::base::{Literal, LiteralKind};
use crate::zig::ZigExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .expect("load Zig grammar");
    let tree = parser.parse(code, None).expect("parse Zig");
    let mut ext = ZigExtractor::new(
        "zig".to_string(),
        "test.zig".to_string(),
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
fn load() void {
    greet("hello");
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
fn std_uri_parse_url_yields_dotted_carrier() {
    // `std.Uri.parse("https://...")` — the callee is a (nested) field_expression;
    // the carrier is the full dotted path so the gate can match `std.uri.parse`
    // exactly or `parse` via the last-segment rule. URL is the only argument.
    let code = r#"
fn load() void {
    _ = std.Uri.parse("https://api.example.com/users");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api.example.com/users")
        .unwrap_or_else(|| panic!("expected the URL literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("std.Uri.parse"),
        "dotted callee carrier is the full path"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn member_callee_sql_yields_dotted_carrier() {
    // `db.exec("SELECT * FROM users")` — zig-sqlite member call. Carrier is the
    // dotted `db.exec` so the gate's last-segment rule matches a bare `exec`.
    let code = r#"
fn load(db: *Db) void {
    db.exec("SELECT * FROM users");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT * FROM users")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("db.exec"),
        "member callee carrier is object.member"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `connect(handle, "https://api.example.com")` — the URL string is the SECOND
    // argument; arg_position is counted over ALL non-callee arguments (the leading
    // `handle` identifier counts), so the URL must report 1, not 0.
    let code = r#"
fn load(handle: *Conn) void {
    connect(handle, "https://api.example.com");
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
    // `print("first", "second")` — the extractor is carrier-AGNOSTIC: it captures
    // BOTH string args (carrier print, positions 0 and 1). Dropping non-carrier
    // literals is the src/ pipeline's job, not the extractor's.
    let code = r#"
fn load() void {
    print("first", "second");
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
            Some("print"),
            "carrier is the callee for every arg"
        );
    }
}
