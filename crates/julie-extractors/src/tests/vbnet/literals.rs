//! VB.NET string-literal call-argument capture (Miller bridge Phase 3).
//!
//! Mirrors the C# reference leg (VB.NET is a sibling .NET language). Extractors
//! capture string literals passed to calls **config-free**: `carrier` is the
//! invoked method name (receiver dropped) and `kind` is always `Other`. URL/SQL
//! classification and the carrier gate are a later `src/` pass. VB wraps each
//! call argument in an `argument` node, and interpolated strings nest as
//! `string_literal > interpolated_string_literal` whose text segments are
//! anonymous tokens — so the leg uses a VB-local interpolation decoder to reach
//! the shared `{}`-hole convention. These tests assert the raw capture across
//! VB's string forms, carrier derivation, `arg_position`, and anchoring.

use crate::base::{Literal, LiteralKind};
use crate::vbnet::VbNetExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_vb_dotnet::LANGUAGE.into())
        .expect("load VB.NET grammar");
    let tree = parser.parse(code, None).expect("parse VB.NET");
    let mut ext = VbNetExtractor::new(
        "vbnet".to_string(),
        "test.vb".to_string(),
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
fn member_call_sql_arg_captured_with_method_carrier() {
    // `conn.Execute("SELECT * FROM Orders")` — member callee, carrier is the
    // method name "Execute" (receiver dropped, mirrors C#). Plain string decoded
    // without quotes; kind=Other; anchored to the enclosing Sub.
    let code = r#"
Class Repo
    Sub Load(conn As IDbConnection)
        Dim rows = conn.Execute("SELECT * FROM Orders")
    End Sub
End Class
"#;
    let literals = capture(code);
    let lit = find(&literals, "FROM Orders");
    assert_eq!(
        lit.literal_text, "SELECT * FROM Orders",
        "plain string body decoded without delimiters"
    );
    assert_eq!(
        lit.carrier.as_deref(),
        Some("Execute"),
        "carrier is the method name (receiver dropped)"
    );
    assert_eq!(lit.arg_position, 0, "first argument");
    assert_eq!(lit.kind, LiteralKind::Other);
    assert!(
        lit.containing_symbol_id.is_some(),
        "literal anchored to the enclosing method symbol"
    );
}

#[test]
fn bare_identifier_callee_yields_name_carrier() {
    // `DoFetch("/api/health")` — a plain identifier callee gives the bare name.
    let code = r#"
Module Api
    Function Load() As String
        Return DoFetch("/api/health")
    End Function
End Module
"#;
    let literals = capture(code);
    let lit = find(&literals, "/api/health");
    assert_eq!(lit.carrier.as_deref(), Some("DoFetch"));
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn interpolated_string_arg_replaces_holes_with_placeholder() {
    // `$"SELECT * FROM Users WHERE Id = {id}"` — the interpolation hole decodes
    // to `{}` (VB nests text as anonymous tokens, handled by the VB-local decoder).
    let code = r#"
Class Repo
    Sub Load(conn As IDbConnection, id As Integer)
        Dim rows = conn.Execute($"SELECT * FROM Users WHERE Id = {id}")
    End Sub
End Class
"#;
    let literals = capture(code);
    let lit = find(&literals, "FROM Users");
    assert_eq!(
        lit.literal_text, "SELECT * FROM Users WHERE Id = {}",
        "interpolation hole -> {{}}, no delimiter leakage"
    );
    assert_eq!(lit.carrier.as_deref(), Some("Execute"));
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `client.Request(42, "/api/x")` — the string is the SECOND argument, so
    // arg_position is counted over ALL args and must be 1, not 0.
    let code = r#"
Class Repo
    Sub Load(client As Object)
        Dim r = client.Request(42, "/api/x")
    End Sub
End Class
"#;
    let literals = capture(code);
    let lit = find(&literals, "/api/x");
    assert_eq!(
        lit.arg_position, 1,
        "string at second position must report arg_position 1"
    );
    assert_eq!(lit.carrier.as_deref(), Some("Request"));
}

#[test]
fn non_string_args_do_not_produce_literals() {
    // `conn.Execute(cmd, 42)` — no string-literal arguments, so no literals.
    let code = r#"
Class Repo
    Sub Load(conn As IDbConnection, cmd As Object)
        Dim r = conn.Execute(cmd, 42)
    End Sub
End Class
"#;
    let literals = capture(code);
    assert!(
        literals.is_empty(),
        "no string-literal args -> no literals, got {literals:?}"
    );
}
