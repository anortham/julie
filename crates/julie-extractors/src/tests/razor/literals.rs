//! Razor string-literal call-argument capture (Miller bridge Phase 3).
//!
//! Razor embeds C# in `@code { }`, so this mirrors the C# reference leg:
//! extractors capture string literals passed to calls **config-free** (carrier =
//! method name with generics stripped, kind = Other). The carrier classification
//! + gate are a later `src/` pass. These tests assert the raw capture across C#'s
//! string forms (plain, verbatim `@"..."`, interpolated `$"...{x}..."` -> `{}`),
//! carrier derivation (bare identifier and member callee), `arg_position`, and
//! enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::razor::RazorExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_razor::LANGUAGE.into())
        .expect("load Razor grammar");
    let tree = parser.parse(code, None).expect("parse Razor");
    let mut ext = RazorExtractor::new(
        "razor".to_string(),
        "test.razor".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_literals()
}

fn find<'a>(literals: &'a [Literal], needle: &str) -> &'a Literal {
    literals
        .iter()
        .find(|l| l.literal_text.contains(needle))
        .unwrap_or_else(|| panic!("expected a literal containing {needle:?}, got {literals:?}"))
}

#[test]
fn dapper_query_string_arg_captured_with_method_carrier() {
    // `conn.Query<User>("SELECT ...")` — SQL captured verbatim, carrier is the
    // method name "Query" (generics stripped, receiver dropped), kind=Other,
    // anchored to the enclosing method.
    let code = r#"@code {
    void Load(IDbConnection conn) {
        var users = conn.Query<User>("SELECT Id, Name FROM Users WHERE Id = @id");
    }
}"#;
    let literals = capture(code);
    let lit = find(&literals, "FROM Users");
    assert_eq!(
        lit.literal_text, "SELECT Id, Name FROM Users WHERE Id = @id",
        "plain string body decoded without delimiters"
    );
    assert_eq!(
        lit.carrier.as_deref(),
        Some("Query"),
        "carrier is the method name with generics stripped"
    );
    assert_eq!(lit.arg_position, 0);
    assert_eq!(lit.kind, LiteralKind::Other);
    assert!(
        lit.containing_symbol_id.is_some(),
        "literal anchored to the enclosing method symbol"
    );
}

#[test]
fn verbatim_string_arg_decoded_without_at_prefix_or_quotes() {
    // `@"SELECT * FROM Orders"` — verbatim_string_literal delimiter-strip removes
    // `@"` and the trailing `"`.
    let code = r#"@code {
    void Load(IDbConnection conn) {
        conn.Execute(@"SELECT * FROM Orders");
    }
}"#;
    let literals = capture(code);
    let lit = find(&literals, "FROM Orders");
    assert_eq!(lit.literal_text, "SELECT * FROM Orders");
    assert_eq!(lit.carrier.as_deref(), Some("Execute"));
}

#[test]
fn interpolated_string_arg_replaces_holes_with_placeholder() {
    // `$"SELECT * FROM {table} WHERE active = 1"` — interpolation hole -> `{}`,
    // no delimiter leakage.
    let code = r#"@code {
    void Load(IDbConnection conn, string table) {
        conn.Query<Row>($"SELECT * FROM {table} WHERE active = 1");
    }
}"#;
    let literals = capture(code);
    let lit = find(&literals, "FROM");
    assert_eq!(
        lit.literal_text, "SELECT * FROM {} WHERE active = 1",
        "interpolation hole -> {{}}, no delimiter leakage"
    );
    assert_eq!(lit.carrier.as_deref(), Some("Query"));
}

#[test]
fn bare_identifier_callee_yields_name_carrier() {
    // `Fetch("/api/health")` — a plain identifier callee gives the bare name.
    let code = r#"@code {
    string Load() {
        return Fetch("/api/health");
    }
}"#;
    let literals = capture(code);
    let lit = find(&literals, "/api/health");
    assert_eq!(lit.carrier.as_deref(), Some("Fetch"));
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `conn.Execute(cmd, "/api/x")` — the string is the SECOND argument; the
    // first (`cmd`) is a non-string, so arg_position must be 1, not 0.
    let code = r#"@code {
    void Load(IDbConnection conn, object cmd) {
        conn.Execute(cmd, "/api/x");
    }
}"#;
    let literals = capture(code);
    let lit = find(&literals, "/api/x");
    assert_eq!(
        lit.arg_position, 1,
        "string at second position must report arg_position 1"
    );
    assert_eq!(lit.carrier.as_deref(), Some("Execute"));
}

#[test]
fn non_string_args_do_not_produce_literals() {
    // `conn.Execute(cmd, 42)` — no string-literal arguments, so no literals.
    let code = r#"@code {
    void Load(IDbConnection conn, object cmd) {
        conn.Execute(cmd, 42);
    }
}"#;
    let literals = capture(code);
    assert!(
        literals.is_empty(),
        "no string-literal args -> no literals, got {literals:?}"
    );
}
