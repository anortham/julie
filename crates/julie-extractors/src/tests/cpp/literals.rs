//! C++ string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Extractors capture string literals passed to calls **config-free**: the
//! `carrier` is the verbatim callee text and `kind` is always `Other` straight
//! from the reader. URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline (`classify_literals_by_carrier`), not here. These tests
//! assert the raw capture: text decoding, carrier derivation (bare,
//! `recv.method` for a member call, and `name` for a `template_function` call),
//! `arg_position` over the full argument list, and enclosing-symbol anchoring.
//! The template-call test guards that capture fires even though the identifier
//! logic returns early for `template_function` callees.

use crate::base::{Literal, LiteralKind};
use crate::cpp::CppExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cpp::LANGUAGE.into())
        .expect("load C++ grammar");
    let tree = parser.parse(code, None).expect("parse C++");
    let mut ext = CppExtractor::new(
        "test.cpp".to_string(),
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
    // (`void` return keeps the function_declarator un-nested so the C++ symbol
    // extractor reliably records the enclosing symbol; a pointer-return free
    // function is wrapped in a `pointer_declarator` the extractor doesn't unwrap.)
    let code = r#"
void load() {
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
fn member_callee_yields_object_method_carrier() {
    // `db.exec("SELECT * FROM users")` — the callee is a field_expression; the
    // carrier must be the `object.method` join so the gate's last-segment rule
    // can match a bare `exec` config.
    let code = r#"
void load(SQLite::Database &db) {
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
        "member callee carrier is object.method"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn bare_template_function_callee_captures_arg_with_name_carrier() {
    // `query<User>("SELECT * FROM users")` — a bare generic call parses as a
    // `template_function` callee. The identifier logic returns early for these,
    // so the literal capture must run BEFORE that early-return; the carrier is the
    // template `name` (generics stripped).
    let code = r#"
void load() {
    query<User>("SELECT * FROM users");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT * FROM users")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("query"),
        "template_function carrier is the name segment"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn member_template_method_strips_generics_from_carrier() {
    // `repo.query<User>("SELECT * FROM users")` — the callee is a field_expression
    // whose `field` is a `template_method` (`query<User>`). The carrier must strip
    // the generic args so the gate's last-segment rule matches a bare `query`
    // config: carrier is `repo.query`, not `repo.query<User>`.
    let code = r#"
void load(Repo &repo) {
    repo.query<User>("SELECT * FROM users");
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT * FROM users")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("repo.query"),
        "member template-method carrier strips generics"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn sqlite_exec_sql_arg_reports_full_arg_position() {
    // `sqlite3_exec(db, "SELECT * FROM users", 0, 0, 0)` — the SQL is the SECOND
    // argument; arg_position is counted over ALL arguments, so it must be 1.
    let code = r#"
void load(sqlite3 *db) {
    sqlite3_exec(db, "SELECT * FROM users", 0, 0, 0);
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT * FROM users")
        .unwrap_or_else(|| panic!("expected the SQL literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("sqlite3_exec"));
    assert_eq!(lit.arg_position, 1, "SQL string is the second argument");
}

#[test]
fn multiple_string_args_each_captured_carrier_agnostic() {
    // `printf("first", "second")` — the extractor is carrier-AGNOSTIC: it
    // captures BOTH string args (carrier printf, positions 0 and 1).
    let code = r#"
void load() {
    printf("first", "second");
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
            Some("printf"),
            "carrier is the callee for every arg"
        );
    }
}
