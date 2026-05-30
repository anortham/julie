//! R string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Like the Python reference leg, the extractor captures string literals passed
//! to `call` nodes **config-free**: the `carrier` is the callee — a bare
//! function name (`dbGetQuery`, imported `POST`), or the `package.function`
//! join for a `namespace_operator` (`httr::GET` → `httr.GET`) / `extract_operator`
//! (`con$query` → `con.query`). `kind` is always `Other`; URL/SQL classification
//! and the carrier gate happen later in the `src/` pipeline. R strings have no
//! interpolation, so decoding is a plain delimiter strip. These tests assert raw
//! capture: carrier derivation (bare AND qualified), `arg_position` over the full
//! list, and enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::r::RExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_r::LANGUAGE.into())
        .expect("load R grammar");
    let tree = parser.parse(code, None).expect("parse R");
    let mut ext = RExtractor::new(
        "r".to_string(),
        "test.R".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.base().literals.clone()
}

#[test]
fn httr_get_namespace_call_captured_with_qualified_carrier() {
    // `httr::GET("https://api/users")` — `namespace_operator`, so the carrier is
    // the `package.function` join `httr.GET`. kind stays Other; the literal
    // anchors to the enclosing function.
    let code = r#"
load <- function() {
  httr::GET("https://api/users")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/users")
        .unwrap_or_else(|| panic!("expected one literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("httr.GET"),
        "namespace callee carrier is package.function"
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
fn dbi_query_sql_arg_captured_at_full_position() {
    // `dbGetQuery(con, "SELECT ... FROM users")` — bare DBI function. The string
    // is the SECOND argument, so arg_position is counted over ALL args and must
    // be 1, not 0.
    let code = r#"
fetch <- function(con) {
  dbGetQuery(con, "SELECT id, name FROM users")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("FROM users"))
        .unwrap_or_else(|| panic!("expected the sql literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("dbGetQuery"));
    assert_eq!(
        lit.arg_position, 1,
        "string at second position must report arg_position 1"
    );
}

#[test]
fn imported_bare_verb_yields_name_carrier() {
    // `POST("https://api/v")` — an imported httr verb is a plain `identifier`
    // callee, so the carrier is the bare name.
    let code = r#"
send <- function() {
  POST("https://api/v")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/v")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("POST"));
}

#[test]
fn extract_operator_call_joins_receiver() {
    // `con$query("SELECT 1")` — `extract_operator` callee, so the carrier is the
    // `con.query` join (last segment `query` for the gate's last-segment rule).
    let code = r#"
run <- function(con) {
  con$query("SELECT 1")
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT 1")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("con.query"));
}
