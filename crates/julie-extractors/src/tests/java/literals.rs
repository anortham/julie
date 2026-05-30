//! Java string-literal call-argument capture (Miller bridge Phase 3b).
//!
//! Like the Python reference leg, the extractor captures string literals passed
//! to `method_invocation` nodes **config-free**: the `carrier` is the verbatim
//! callee — the bare `name` for a receiverless call, or the `object.name` join
//! for a member call (`restTemplate.getForObject`, `st.execute`). `kind` is
//! always `Other`; URL/SQL classification and the carrier gate happen later in
//! the `src/` pipeline. These tests assert raw capture: carrier derivation (bare
//! AND dotted), `arg_position` over the full list, `kind == Other`, and
//! enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::java::JavaExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .expect("load Java grammar");
    let tree = parser.parse(code, None).expect("parse Java");
    let mut ext = JavaExtractor::new(
        "java".to_string(),
        "Test.java".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.base().literals.clone()
}

#[test]
fn rest_template_get_captured_with_dotted_carrier() {
    // `restTemplate.getForObject("https://api/users", String.class)` — member
    // callee, so the carrier is the `object.name` join. kind stays Other; the
    // literal anchors to the enclosing method.
    let code = r#"
class C {
    void load(RestTemplate restTemplate) {
        restTemplate.getForObject("https://api/users", String.class);
    }
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://api/users")
        .unwrap_or_else(|| panic!("expected one literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("restTemplate.getForObject"),
        "dotted callee carrier is object.name"
    );
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
fn statement_execute_sql_arg_captured() {
    // `st.execute("SELECT ... FROM users")`. The carrier `st.execute` is captured
    // verbatim; the gate later matches the bare `execute` config by last segment.
    let code = r#"
class C {
    void fetch(Statement st) {
        st.execute("SELECT id, name FROM users");
    }
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("FROM users"))
        .unwrap_or_else(|| panic!("expected the sql literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("st.execute"));
}

#[test]
fn bare_method_callee_yields_name_carrier() {
    // `execute("SELECT 1")` — a receiverless (same-class) method call gives the
    // bare method name.
    let code = r#"
class C {
    void run() {
        execute("SELECT 1");
    }
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT 1")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("execute"));
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `helper.call(42, "/api/x")` — the string is the SECOND argument, so
    // arg_position is counted over ALL args and must be 1, not 0.
    let code = r#"
class C {
    void load(Helper helper) {
        helper.call(42, "/api/x");
    }
}
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
    assert_eq!(lit.carrier.as_deref(), Some("helper.call"));
}