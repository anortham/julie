//! Lua string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Like the Python reference leg, the extractor captures string literals passed
//! to `function_call` nodes **config-free**: the `carrier` is the verbatim
//! callee — a bare `identifier` (`load`), or the `table.field`/`table.method`
//! join for a `dot_index_expression` (`http.request`) / `method_index_expression`
//! (`conn:execute` → `conn.execute`). `kind` is always `Other`; URL/SQL
//! classification and the carrier gate happen later in the `src/` pipeline. Lua
//! string literals have no interpolation, so decoding is a plain delimiter strip.
//! These tests assert raw capture: carrier derivation (bare AND dotted),
//! `arg_position` over the full list, and enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::lua::LuaExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .expect("load Lua grammar");
    let tree = parser.parse(code, None).expect("parse Lua");
    let mut ext = LuaExtractor::new(
        "lua".to_string(),
        "test.lua".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.base().literals.clone()
}

#[test]
fn http_request_dot_call_captured_with_dotted_carrier() {
    // `http.request("https://api/users")` — `dot_index_expression` name, so the
    // carrier is the `table.field` join `http.request`. kind stays Other; the
    // literal anchors to the enclosing function.
    let code = r#"
function load()
  http.request("https://api/users")
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/users")
        .unwrap_or_else(|| panic!("expected one literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("http.request"),
        "dot_index_expression carrier is table.field"
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
fn method_call_execute_sql_arg_captured() {
    // `conn:execute("SELECT ... FROM users")` — `method_index_expression` name,
    // so the carrier is `conn.execute`; the gate later matches the bare `execute`
    // config by last segment.
    let code = r#"
function fetch(conn)
  conn:execute("SELECT id, name FROM users")
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("FROM users"))
        .unwrap_or_else(|| panic!("expected the sql literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("conn.execute"));
}

#[test]
fn bare_identifier_callee_yields_name_carrier() {
    // `load("config.lua")` — a plain identifier callee gives the bare name.
    let code = r#"
function init()
  load("config.lua")
end
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "config.lua")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("load"));
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `client.request(42, "/api/x")` — the string is the SECOND argument, so
    // arg_position is counted over ALL args and must be 1, not 0.
    let code = r#"
function load(client)
  client.request(42, "/api/x")
end
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
    assert_eq!(lit.carrier.as_deref(), Some("client.request"));
}
