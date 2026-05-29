//! C# string-literal call-argument capture (Miller bridge Phase 3).
//!
//! Mirrors the TS reference leg: extractors capture string literals passed to
//! calls **config-free** (carrier = method name, kind = Other). The carrier
//! classification + gate are a later `src/` pass. These tests assert the raw
//! capture across C#'s string forms: plain `string_literal`, `verbatim_string_literal`
//! (`@"..."`), `raw_string_literal` (`"""..."""`), and `interpolated_string_expression`
//! (`$"...{x}..."` -> `{}`). C# wraps each call argument in an `argument` node, so
//! the capture descends one level the TS leg does not.

use crate::base::{Literal, LiteralKind};
use crate::csharp::CSharpExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .expect("load C# grammar");
    let tree = parser.parse(code, None).expect("parse C#");
    let mut ext = CSharpExtractor::new(
        "csharp".to_string(),
        "test.cs".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_base().literals.clone()
}

fn find<'a>(literals: &'a [Literal], needle: &str) -> &'a Literal {
    literals
        .iter()
        .find(|l| l.literal_text.contains(needle))
        .unwrap_or_else(|| panic!("expected a literal containing {needle:?}, got {literals:?}"))
}

#[test]
fn dapper_query_string_arg_captured_with_method_carrier() {
    // `conn.Query<User>("SELECT Id, Name FROM Users WHERE Id = @id")` — the SQL
    // body is captured verbatim, carrier is the method name "Query" (generics
    // stripped, receiver dropped), kind=Other (Sql classification is a src/ pass).
    let code = r#"
class Repo {
    void Load(IDbConnection conn) {
        var users = conn.Query<User>("SELECT Id, Name FROM Users WHERE Id = @id");
    }
}
"#;
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
    assert_eq!(lit.kind, LiteralKind::Other);
    assert!(lit.containing_symbol_id.is_some());
}

#[test]
fn verbatim_string_arg_decoded_without_at_prefix_or_quotes() {
    // `@"SELECT * FROM Orders"` — verbatim_string_literal is a single token with
    // no inner named children; the delimiter-strip fallback must remove `@"` and
    // the trailing `"`.
    let code = r#"
class Repo {
    void Load(IDbConnection conn) {
        conn.Execute(@"SELECT * FROM Orders");
    }
}
"#;
    let literals = capture(code);
    let lit = find(&literals, "FROM Orders");
    assert_eq!(lit.literal_text, "SELECT * FROM Orders");
    assert_eq!(lit.carrier.as_deref(), Some("Execute"));
}

#[test]
fn interpolated_string_arg_replaces_holes_with_placeholder() {
    // `$"SELECT * FROM {table}"` — the interpolation hole is decoded to `{}` and
    // the interpolation_start/quote delimiters must NOT leak into the text.
    let code = r#"
class Repo {
    void Load(IDbConnection conn, string table) {
        conn.Query<Row>($"SELECT * FROM {table} WHERE active = 1");
    }
}
"#;
    let literals = capture(code);
    let lit = find(&literals, "FROM");
    assert_eq!(
        lit.literal_text, "SELECT * FROM {} WHERE active = 1",
        "interpolation hole -> {{}}, no delimiter leakage"
    );
    assert_eq!(lit.carrier.as_deref(), Some("Query"));
}

#[test]
fn raw_string_arg_decoded_without_triple_quotes() {
    // `"""SELECT * FROM Logs"""` — raw_string_literal exposes named
    // raw_string_start/end (the `"""`); only raw_string_content is text.
    let code = r#"
class Repo {
    void Load(IDbConnection conn) {
        conn.Execute("""SELECT * FROM Logs""");
    }
}
"#;
    let literals = capture(code);
    let lit = find(&literals, "FROM Logs");
    assert_eq!(
        lit.literal_text, "SELECT * FROM Logs",
        "raw string triple-quote delimiters stripped"
    );
}

#[test]
fn non_string_args_do_not_produce_literals() {
    // `conn.Execute(cmd, 42)` — no string-literal arguments, so no literals.
    let code = r#"
class Repo {
    void Load(IDbConnection conn, object cmd) {
        conn.Execute(cmd, 42);
    }
}
"#;
    let literals = capture(code);
    assert!(
        literals.is_empty(),
        "no string-literal args -> no literals, got {literals:?}"
    );
}
