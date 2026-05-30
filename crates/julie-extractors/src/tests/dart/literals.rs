//! Dart string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding, carrier derivation (bare + dotted
//! `db.rawQuery`), named-argument value descent, `arg_position`, and
//! enclosing-symbol anchoring.
//!
//! NOTE: Dart interpolation (`$x`) nests text as `template_chars_*` which the
//! shared decoder doesn't recognize, so no `{}` normalization is asserted here
//! (flagged to the lead as a base-decoder gap). Plain literals decode via the
//! delimiter-strip fallback.

use crate::base::{Literal, LiteralKind};
use crate::dart::DartExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_dart::LANGUAGE.into())
        .expect("load Dart grammar");
    let tree = parser.parse(code, None).expect("parse Dart");
    let mut ext = DartExtractor::new(
        "dart".to_string(),
        "test.dart".to_string(),
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
class Loader {
  String load() {
    return greet("hello");
  }
}
"#;
    let literals = capture(code);
    let hits: Vec<&Literal> = literals
        .iter()
        .filter(|l| l.literal_text == "hello")
        .collect();
    assert_eq!(hits.len(), 1, "exactly one literal for the arg, got {literals:?}");
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
        "literal anchored to the enclosing method symbol"
    );
}

#[test]
fn dotted_member_callee_yields_object_property_carrier() {
    // `db.rawQuery("SELECT * FROM users")` — the callee is a member_expression;
    // the carrier must be the `object.property` join so config can match the
    // bare DB verb `rawQuery` via the last-segment rule.
    let code = r#"
class Repo {
  void all() {
    db.rawQuery("SELECT * FROM users");
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
        Some("db.rawQuery"),
        "member-call carrier is object.property"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn named_argument_value_is_captured() {
    // `http.post(url, body: "payload")` — the string is the `value` of a named
    // argument at the SECOND position; the descent past the `body:` label must
    // still capture it, and arg_position is 1 (counted over the full list).
    let code = r#"
class Client {
  void send(String url) {
    http.post(url, body: "payload");
  }
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "payload")
        .unwrap_or_else(|| panic!("expected the named-arg literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("http.post"),
        "member-call carrier is object.property"
    );
    assert_eq!(
        lit.arg_position, 1,
        "named arg is the second argument in the full list"
    );
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `connect("localhost", 5432, "mydb")` — the "mydb" string is the THIRD
    // argument; arg_position is counted over ALL arguments, so it must be 2.
    let code = r#"
class Db {
  void open() {
    connect("localhost", 5432, "mydb");
  }
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "mydb")
        .unwrap_or_else(|| panic!("expected the db-name literal, got {literals:?}"));
    assert_eq!(
        lit.arg_position, 2,
        "string at third position must report arg_position 2"
    );
}

#[test]
fn multiple_string_args_each_captured_carrier_agnostic() {
    // `log("first", "second")` — the extractor is carrier-AGNOSTIC: it captures
    // BOTH string args (carrier "log", positions 0 and 1). Dropping non-carrier
    // literals is the src/ pipeline's job, not the extractor's.
    let code = r#"
class L {
  void run() {
    log("first", "second");
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
            Some("log"),
            "carrier is the bare callee for every arg"
        );
    }
}
