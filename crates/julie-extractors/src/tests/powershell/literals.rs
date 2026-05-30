//! PowerShell string-literal command-argument capture (Miller bridge Phase 3b).
//!
//! PowerShell is a COMMAND grammar, not `call_expression`: a `command` node has a
//! `command_name` field (the cmdlet) and a `command_elements` field holding the
//! argument list (whitespace separators, `command_parameter` flags like
//! `-Uri`/`-Query`, and value expressions). String values are nested
//! (`array_literal_expression > unary_expression > string_literal`). The
//! extractor captures them **config-free** — the `carrier` is the verbatim cmdlet
//! name and `kind` is always `Other`; URL/SQL classification and the carrier gate
//! happen later in the `src/` pipeline (`classify_literals_by_carrier`).
//!
//! These tests assert the raw capture: expandable (double-quote) and verbatim
//! (single-quote) decoding, cmdlet-name carrier, `arg_position` over the
//! non-separator element list (a `-Uri`/`-Query` flag occupies a position, so the
//! quoted value reports the next index; a positional value reports 0), and
//! enclosing-symbol anchoring.

use crate::base::{Literal, LiteralKind};
use crate::powershell::PowerShellExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_powershell::LANGUAGE.into())
        .expect("load PowerShell grammar");
    let tree = parser.parse(code, None).expect("parse PowerShell");
    let mut ext = PowerShellExtractor::new(
        "powershell".to_string(),
        "test.ps1".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_literals()
}

#[test]
fn named_param_arg_captured_with_cmdlet_carrier() {
    // `Invoke-RestMethod -Uri "https://…"` — the quoted URL value follows the
    // `-Uri` flag. Recorded verbatim with carrier="Invoke-RestMethod" (the cmdlet
    // name), kind=Other, anchored to the enclosing function. arg_position is 1
    // because the `-Uri` flag occupies position 0.
    let code = r#"
function Get-Data {
    Invoke-RestMethod -Uri "https://api.example.com/users"
}
"#;
    let literals = capture(code);
    let hits: Vec<&Literal> = literals
        .iter()
        .filter(|l| l.literal_text == "https://api.example.com/users")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "exactly one literal for the arg, got {literals:?}"
    );
    let lit = hits[0];
    assert_eq!(
        lit.carrier.as_deref(),
        Some("Invoke-RestMethod"),
        "carrier is the cmdlet name"
    );
    assert_eq!(
        lit.arg_position, 1,
        "the -Uri flag is position 0, so its value is position 1"
    );
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
fn positional_arg_reports_position_zero() {
    // `Invoke-WebRequest "https://…"` — a positional (unnamed) value is the first
    // element, so arg_position is 0.
    let code = r#"
function Fetch-Page {
    Invoke-WebRequest "https://example.com/page"
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "https://example.com/page")
        .unwrap_or_else(|| panic!("expected the positional URL literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("Invoke-WebRequest"),
        "carrier is the cmdlet name"
    );
    assert_eq!(lit.arg_position, 0, "positional value is the first element");
}

#[test]
fn verbatim_single_quoted_string_decodes_to_contents() {
    // `Invoke-Sqlcmd -Query 'SELECT 1'` — a verbatim (single-quoted) string.
    // decode_string_literal must strip the single quotes and keep the SQL
    // verbatim, carrier="Invoke-Sqlcmd".
    let code = r#"
function Run-Query {
    Invoke-Sqlcmd -Query 'SELECT id FROM Users'
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT id FROM Users")
        .unwrap_or_else(|| panic!("expected the verbatim SQL literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("Invoke-Sqlcmd"),
        "carrier is the cmdlet name"
    );
    assert_eq!(
        lit.arg_position, 1,
        "the -Query flag is position 0, so its value is position 1"
    );
    assert_eq!(lit.kind, LiteralKind::Other);
}

#[test]
fn multiple_string_args_each_captured_carrier_agnostic() {
    // `Invoke-RestMethod -Uri "url" -Body "payload"` — the extractor is
    // carrier-AGNOSTIC: it captures BOTH string values under carrier
    // "Invoke-RestMethod". Dropping non-matching literals is the src/ pipeline's
    // job, not the extractor's. -Uri value is position 1, -Body value position 3.
    let code = r#"
function Post-Data {
    Invoke-RestMethod -Uri "https://api.example.com/post" -Body "payload-data"
}
"#;
    let literals = capture(code);
    let texts: Vec<&str> = literals.iter().map(|l| l.literal_text.as_str()).collect();
    assert!(
        texts.contains(&"https://api.example.com/post") && texts.contains(&"payload-data"),
        "both string values captured at the extractor layer, got {texts:?}"
    );
    for l in literals.iter().filter(|l| {
        l.literal_text == "https://api.example.com/post" || l.literal_text == "payload-data"
    }) {
        assert_eq!(
            l.carrier.as_deref(),
            Some("Invoke-RestMethod"),
            "carrier is the cmdlet name for every string value"
        );
    }
    let body = literals
        .iter()
        .find(|l| l.literal_text == "payload-data")
        .unwrap();
    assert_eq!(
        body.arg_position, 3,
        "-Body value follows -Uri value: positions are -Uri(0) url(1) -Body(2) payload(3)"
    );
}

#[test]
fn expandable_string_with_interpolation_is_normalized_to_placeholder() {
    // `Invoke-RestMethod -Uri "https://api.example.com/users/$id"` — an expandable
    // string with a `$id` `variable` subnode. PowerShell tokenizes the static text
    // as anonymous bytes (not a child node), so the cmdlet arm reconstructs the
    // static shape and normalizes the variable to a `{}` placeholder — consistent
    // with bash/Swift/Dart interpolation handling.
    let code = r#"
function Get-User {
    Invoke-RestMethod -Uri "https://api.example.com/users/$id"
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.starts_with("https://api.example.com/users/"))
        .unwrap_or_else(|| panic!("expected the interpolated URL literal, got {literals:?}"));
    assert_eq!(
        lit.literal_text, "https://api.example.com/users/{}",
        "variable expansion must normalize to a {{}} placeholder"
    );
    assert_eq!(
        lit.carrier.as_deref(),
        Some("Invoke-RestMethod"),
        "carrier is the cmdlet name"
    );
}

#[test]
fn subexpression_in_expandable_string_is_normalized_to_placeholder() {
    // `"https://api/users/$($u.Id)/x"` — a `$(...)` `sub_expression` is a single
    // hole; the whole `$($u.Id)` collapses to one `{}`, surrounding text intact.
    let code = r#"
function Get-User {
    Invoke-RestMethod -Uri "https://api/users/$($u.Id)/x"
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.starts_with("https://api/users/"))
        .unwrap_or_else(|| panic!("expected the sub-expression URL literal, got {literals:?}"));
    assert_eq!(
        lit.literal_text, "https://api/users/{}/x",
        "a $(...) sub-expression collapses to a single {{}} placeholder"
    );
}
