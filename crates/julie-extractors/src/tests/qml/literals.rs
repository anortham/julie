//! QML string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding, carrier derivation (bare +
//! `object.property` for a member call), `arg_position` over the full argument
//! list, and enclosing-symbol anchoring. The carriers exercised are QML-JS APIs
//! (`XMLHttpRequest.open`, Qt LocalStorage `tx.executeSql`), not browser/Node
//! HTTP libraries, which QML's JS engine does not provide.

use crate::base::{Literal, LiteralKind};
use crate::qml::QmlExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_qmljs::LANGUAGE.into())
        .expect("load QML grammar");
    let tree = parser.parse(code, None).expect("parse QML");
    let mut ext = QmlExtractor::new(
        "qml".to_string(),
        "test.qml".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_literals()
}

#[test]
fn bare_function_call_arg_captured_with_carrier() {
    // `greet("hello")` inside a QML function — plain-identifier callee, recorded
    // verbatim with carrier="greet", arg_position=0, kind=Other, anchored to the
    // enclosing function symbol.
    let code = r#"
import QtQuick 2.15

Item {
    function load() {
        greet("hello");
    }
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
fn xhr_open_url_reports_full_arg_position() {
    // `xhr.open("GET", "https://...")` — the URL is the SECOND argument; the
    // callee is a member_expression so the carrier is `xhr.open`, and arg_position
    // is counted over ALL arguments, so the URL reports 1.
    let code = r#"
import QtQuick 2.15

Item {
    function load(xhr) {
        xhr.open("GET", "https://api.example.com/users");
    }
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api.example.com/users")
        .unwrap_or_else(|| panic!("expected the URL literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("xhr.open"),
        "member callee carrier is object.property"
    );
    assert_eq!(lit.arg_position, 1, "URL string is the second argument");
}

#[test]
fn execute_sql_member_callee_yields_object_property_carrier() {
    // `tx.executeSql("SELECT * FROM users")` — Qt LocalStorage transaction API.
    // The carrier is the `object.property` join so the gate's last-segment rule
    // can match a bare `executeSql` config.
    let code = r#"
import QtQuick 2.15

Item {
    function load(tx) {
        tx.executeSql("SELECT * FROM users");
    }
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT * FROM users")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("tx.executeSql"),
        "member callee carrier is object.property"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn multiple_string_args_each_captured_carrier_agnostic() {
    // `console.log("first", "second")` — the extractor is carrier-AGNOSTIC: it
    // captures BOTH string args (carrier console.log, positions 0 and 1).
    let code = r#"
import QtQuick 2.15

Item {
    function load() {
        console.log("first", "second");
    }
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
