//! Tests for bracket balance validation and diff formatting.

use crate::tools::editing::validation::{check_bracket_balance, format_unified_diff, should_check_balance};

#[test]
fn test_balanced_code_passes() {
    let code = "fn main() {\n    let x = vec![1, 2, 3];\n    println!(\"{:?}\", x);\n}\n";
    assert!(check_bracket_balance(code).is_ok());
}

#[test]
fn test_unmatched_open_brace_fails() {
    let code = "fn main() {\n    let x = 1;\n";
    assert!(check_bracket_balance(code).is_err());
}

#[test]
fn test_unmatched_close_paren_fails() {
    let code = "fn main() {\n    let x = foo());\n}\n";
    assert!(check_bracket_balance(code).is_err());
}

#[test]
fn test_empty_string_passes() {
    assert!(check_bracket_balance("").is_ok());
}

#[test]
fn test_should_check_balance_code_files() {
    assert!(should_check_balance("src/main.rs"));
    assert!(should_check_balance("app.py"));
    assert!(should_check_balance("index.ts"));
}

#[test]
fn test_should_check_balance_skips_non_code() {
    assert!(!should_check_balance("README.md"));
    assert!(!should_check_balance("config.yaml"));
    assert!(!should_check_balance("data.json"));
    assert!(!should_check_balance("settings.toml"));
}

#[test]
fn test_unified_diff_format() {
    let before = "line1\nline2\nline3\n";
    let after = "line1\nmodified\nline3\n";
    let diff = format_unified_diff(before, after, "test.rs");
    assert!(diff.contains("--- test.rs"), "Should have before header");
    assert!(diff.contains("+++ test.rs"), "Should have after header");
    assert!(diff.contains("-line2"), "Should show removed line");
    assert!(diff.contains("+modified"), "Should show added line");
}
