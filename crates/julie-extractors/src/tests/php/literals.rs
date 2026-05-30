//! PHP string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Like the Python reference leg, the extractor captures string literals passed
//! to PHP call nodes **config-free**: the `carrier` is the verbatim callee — the
//! bare `function` name for a `function_call_expression` (`mysqli_query`), the
//! `object.name` join for a `member_call_expression` (`$pdo.query`), or the
//! `scope.name` join for a `scoped_call_expression` (`Http.get`). `kind` is always
//! `Other`; URL/SQL classification and the carrier gate happen later in the `src/`
//! pipeline. These tests assert raw capture: carrier derivation (bare, object, and
//! scope shapes), `arg_position` over the full list, `kind == Other`, and
//! enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::php::PhpExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .expect("load PHP grammar");
    let tree = parser.parse(code, None).expect("parse PHP");
    let mut ext = PhpExtractor::new(
        "php".to_string(),
        "test.php".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_literals()
}

#[test]
fn pdo_query_member_call_captured_with_object_carrier() {
    // `$pdo->query("SELECT ... FROM users")` — member callee, so the carrier is
    // the `object.name` join `$pdo.query`; the gate later matches the bare `query`
    // config by last segment. kind stays Other; literal anchors to the function.
    let code = r#"<?php
function fetch($pdo) {
    $pdo->query("SELECT id, name FROM users");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("FROM users"))
        .unwrap_or_else(|| panic!("expected the sql literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("$pdo.query"),
        "member callee carrier is object.name"
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
fn http_facade_static_call_captured_with_scope_carrier() {
    // `Http::get("https://api/v")` — Laravel facade, a `scoped_call_expression`, so
    // the carrier is the `scope.name` join `Http.get` matched by a dotted config.
    let code = r#"<?php
function load() {
    Http::get("https://api/v");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/v")
        .unwrap_or_else(|| panic!("expected the url literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("Http.get"),
        "scoped callee carrier is scope.name"
    );
    assert_eq!(lit.arg_position, 0, "first argument");
}

#[test]
fn procedural_mysqli_query_yields_bare_function_carrier() {
    // `mysqli_query($conn, "SELECT 1")` — a `function_call_expression` gives the
    // bare function name; the SQL is the SECOND argument, so arg_position is 1.
    let code = r#"<?php
function run($conn) {
    mysqli_query($conn, "SELECT 1");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT 1")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("mysqli_query"));
    assert_eq!(
        lit.arg_position, 1,
        "string at second position must report arg_position 1"
    );
}

#[test]
fn guzzle_request_verb_then_url_captured_at_second_position() {
    // `$client->request("GET", "https://api/u")` — Guzzle's verb-first form. The
    // carrier is `$client.request` (last segment `request`), and the URL literal
    // sits at arg_position 1 behind the HTTP-verb string at 0.
    let code = r#"<?php
function call($client) {
    $client->request("GET", "https://api/u");
}
"#;
    let literals = capture(code);
    let url = literals
        .iter()
        .find(|l| l.literal_text == "https://api/u")
        .unwrap_or_else(|| panic!("expected the url literal, got {literals:?}"));
    assert_eq!(url.carrier.as_deref(), Some("$client.request"));
    assert_eq!(url.arg_position, 1, "url is the second argument");
    // The HTTP-verb string at position 0 is captured too (config-free); the carrier
    // gate keeps both since `request` is a configured carrier.
    let verb = literals
        .iter()
        .find(|l| l.literal_text == "GET")
        .expect("verb string captured at position 0");
    assert_eq!(verb.arg_position, 0);
}
