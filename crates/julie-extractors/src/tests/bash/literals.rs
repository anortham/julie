//! Bash string-literal command-argument capture (Miller bridge Phase 3b).
//!
//! Bash is a COMMAND grammar, not `call_expression`: a `command` node has a
//! `name` field (the `command_name`) and repeated `argument`-field children.
//! The extractor captures string-literal args **config-free** — the `carrier`
//! is the verbatim command name and `kind` is always `Other` straight from the
//! reader. URL/SQL classification and the carrier gate happen later in the
//! `src/` pipeline (`classify_literals_by_carrier`), keyed off the command name
//! (`curl`/`wget` → URL, `psql`/`mysql`/`sqlite3` → SQL).
//!
//! These tests assert the raw capture: double-quoted (`string`) and
//! single-quoted (`raw_string`) decoding, command-name carrier, `arg_position`
//! over the full argument list (so `psql -c "…"` reports the SQL at 1), enclosing
//! -symbol anchoring, and the deliberate scope boundary that bare unquoted
//! `word` args (an unquoted `curl https://x` URL) are NOT string literals and are
//! not captured.

use crate::base::{Literal, LiteralKind};
use crate::bash::BashExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<Literal> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .expect("load Bash grammar");
    let tree = parser.parse(code, None).expect("parse Bash");
    let mut ext = BashExtractor::new(
        "bash".to_string(),
        "test.sh".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_literals()
}

#[test]
fn quoted_command_arg_captured_with_command_name_carrier() {
    // `curl "https://…"` — the quoted URL is a `string` arg. Recorded verbatim
    // with carrier="curl" (the command name), arg_position=0, kind=Other, and
    // anchored to the enclosing function symbol.
    let code = r#"
deploy() {
    curl "https://api.example.com/users"
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
        Some("curl"),
        "carrier is the command name"
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
fn single_quoted_raw_string_decodes_to_verbatim_contents() {
    // `wget '…'` — a single-quoted `raw_string` arg. decode_string_literal must
    // strip the single quotes and keep the URL verbatim, carrier="wget".
    let code = r#"
fetch() {
    wget 'http://example.com/file.tar.gz'
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "http://example.com/file.tar.gz")
        .unwrap_or_else(|| panic!("expected the raw-string URL literal, got {literals:?}"));
    assert_eq!(lit.carrier.as_deref(), Some("wget"), "command-name carrier");
    assert_eq!(lit.kind, LiteralKind::Other);
}

#[test]
fn arg_position_counts_full_argument_list() {
    // `psql -c "SELECT …"` — args are [`-c` (a word), the SQL string]. The SQL
    // string is the SECOND argument, so arg_position must be 1; the `-c` word is
    // not a string literal and is skipped (but still occupies position 0).
    let code = r#"
query() {
    psql -c "SELECT id FROM users"
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text == "SELECT id FROM users")
        .unwrap_or_else(|| panic!("expected the SQL literal, got {literals:?}"));
    assert_eq!(
        lit.carrier.as_deref(),
        Some("psql"),
        "carrier is the command name"
    );
    assert_eq!(
        lit.arg_position, 1,
        "SQL string is the second argument; the `-c` word is position 0"
    );
}

#[test]
fn multiple_string_args_each_captured_carrier_agnostic() {
    // `psql -c "SELECT 1" -c "SELECT 2"` — the extractor is carrier-AGNOSTIC: it
    // captures BOTH string args under carrier "psql". Dropping non-matching
    // literals is the src/ pipeline's job, not the extractor's.
    let code = r#"
multi() {
    psql -c "SELECT 1" -c "SELECT 2"
}
"#;
    let literals = capture(code);
    let texts: Vec<&str> = literals.iter().map(|l| l.literal_text.as_str()).collect();
    assert!(
        texts.contains(&"SELECT 1") && texts.contains(&"SELECT 2"),
        "both string args captured at the extractor layer, got {texts:?}"
    );
    for l in literals
        .iter()
        .filter(|l| l.literal_text.starts_with("SELECT"))
    {
        assert_eq!(
            l.carrier.as_deref(),
            Some("psql"),
            "carrier is the command name for every string arg"
        );
    }
}

#[test]
fn expansion_in_quoted_arg_is_normalized_to_placeholder() {
    // `curl "https://api/users/$id"` — a double-quoted `string` whose `$id` parses
    // as a `simple_expansion` named child (and `${id}` as `expansion`). The static
    // `string_content` text must be preserved AND the expansion normalized to a
    // `{}` placeholder, so the resolver sees `https://api/users/{}` — NOT a
    // truncated `https://api/users/` (the expansion silently dropped).
    let code = r#"
deploy() {
    curl "https://api/users/$id"
}
"#;
    let literals = capture(code);
    let lit = literals
        .iter()
        .find(|l| l.literal_text.starts_with("https://api/users/"))
        .unwrap_or_else(|| panic!("expected the expansion URL literal, got {literals:?}"));
    assert_eq!(
        lit.literal_text, "https://api/users/{}",
        "expansion must normalize to a {{}} placeholder, not be dropped"
    );
    assert_eq!(lit.carrier.as_deref(), Some("curl"), "command-name carrier");
}

#[test]
fn bare_word_url_argument_is_not_captured() {
    // `curl https://bare.example.com` — the URL is an unquoted `word`, not a
    // string literal. The string-literal contract is consistent across all
    // languages: only quoted args are captured. The bare word must NOT appear as
    // a literal (quoting is required).
    let code = r#"
bare() {
    curl https://bare.example.com
}
"#;
    let literals = capture(code);
    assert!(
        !literals
            .iter()
            .any(|l| l.literal_text.contains("bare.example.com")),
        "bare unquoted word URL must not be captured as a string literal, got {literals:?}"
    );
}
