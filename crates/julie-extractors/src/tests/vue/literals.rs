//! Vue string-literal call-argument capture (Miller bridge Phase 3).
//!
//! Vue `<script>` blocks are JavaScript/TypeScript, parsed with their own
//! tree-sitter pass whose byte offsets index the section text. The capture leg
//! decodes from the script content and remaps spans to the host SFC. Capture is
//! **config-free**: `carrier` is the verbatim callee text, `kind` is always
//! `Other`; URL/SQL classification and the carrier gate are a later `src/` pass.
//! These tests assert the raw capture: text decoding (incl. template
//! interpolation holes), carrier derivation (bare `fetch`, dotted `axios.get`),
//! `arg_position` over the full list, and enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::vue::VueExtractor;
use std::path::PathBuf;

fn capture(code: &str) -> Vec<Literal> {
    let mut ext = VueExtractor::new(
        "vue".to_string(),
        "test.vue".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(None);
    ext.extract_identifiers(&symbols);
    ext.get_literals()
}

#[test]
fn fetch_string_arg_captured_with_bare_carrier() {
    // `fetch("/api/users")` in a <script> block — carrier="fetch", arg_position=0,
    // kind=Other, anchored to a containing symbol (the component / function).
    let code = r#"
<script>
function load() {
    return fetch("/api/users");
}
</script>
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "/api/users")
        .unwrap_or_else(|| panic!("expected the fetch literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("fetch"), "bare callee carrier");
    assert_eq!(lit.arg_position, 0, "first argument");
    assert_eq!(
        lit.kind,
        LiteralKind::Other,
        "extractor emits Other; carrier classification is a src/ pass"
    );
    assert!(
        lit.containing_symbol_id.is_some(),
        "literal anchored to a containing symbol"
    );
}

#[test]
fn dotted_member_callee_yields_object_property_carrier() {
    // `axios.get("/api/users")` — member callee, so the carrier is the
    // `object.property` join `axios.get`.
    let code = r#"
<script>
function load() {
    return axios.get("/api/users");
}
</script>
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "/api/users")
        .unwrap_or_else(|| panic!("expected a literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("axios.get"),
        "dotted callee carrier is object.property"
    );
    assert_eq!(lit.arg_position, 0);
}

#[test]
fn template_string_arg_decodes_interpolation_holes() {
    // `fetch(`/api/users/${id}/orders`)` — the template substitution is decoded
    // to a `{}` placeholder. This exercises the Vue-local decoder reading from
    // the script-section bytes.
    let code = r#"
<script>
function load(id) {
    return fetch(`/api/users/${id}/orders`);
}
</script>
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.contains("/api/users/"))
        .unwrap_or_else(|| panic!("expected the template literal, got {literals:?}"));
    assert_eq!(
        lit.literal_text, "/api/users/{}/orders",
        "interpolation hole replaced by {{}}"
    );
    assert_eq!(lit.carrier.as_deref(), Some("fetch"));
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `request(42, "/api/x")` — the string is the SECOND argument, so
    // arg_position is counted over ALL args and must be 1, not 0.
    let code = r#"
<script>
function load() {
    return request(42, "/api/x");
}
</script>
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
}
