//! Tests for bracket balance validation and diff formatting.

use crate::tools::editing::validation::{check_bracket_balance, format_unified_diff, should_check_balance};

#[test]
fn test_balanced_edit_passes() {
    let before = "fn main() {\n    println!(\"hello\");\n}\n";
    let after = "fn main() {\n    println!(\"world\");\n}\n";
    assert!(check_bracket_balance(before, after).is_ok());
}

#[test]
fn test_edit_removing_close_brace_fails() {
    let before = "fn main() {\n    let x = 1;\n}\n";
    let after = "fn main() {\n    let x = 1;\n";
    assert!(check_bracket_balance(before, after).is_err());
}

#[test]
fn test_edit_adding_extra_close_paren_fails() {
    let before = "fn main() {\n    foo();\n}\n";
    let after = "fn main() {\n    foo());\n}\n";
    assert!(check_bracket_balance(before, after).is_err());
}

#[test]
fn test_empty_to_empty_passes() {
    assert!(check_bracket_balance("", "").is_ok());
}

#[test]
fn test_string_literals_with_brackets_preserved() {
    // Both before and after have "unbalanced" brackets inside string literals.
    // The delta is zero, so the edit should pass.
    let before = "let s = \"foo())\";\nlet t = \"bar{\";\n";
    let after = "let s = \"baz())\";\nlet t = \"bar{\";\n";
    assert!(
        check_bracket_balance(before, after).is_ok(),
        "Edit preserving bracket balance in string literals should pass"
    );
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

#[test]
fn test_unified_diff_insertion_does_not_cascade() {
    // When a line is inserted in the middle, only the insertion should appear in the diff,
    // not every subsequent line. This catches the naive index-comparison bug where
    // before[i] vs after[i] misaligns after an insertion.
    let before = "line1\nline2\nline3\nline4\nline5\n";
    let after = "line1\nline2\nINSERTED\nline3\nline4\nline5\n";
    let diff = format_unified_diff(before, after, "test.rs");

    assert!(diff.contains("+INSERTED"), "Should show the inserted line");

    // The diff should NOT show line3/line4/line5 as removed+re-added.
    // If it contains "-line3" then the diff is cascading every subsequent line.
    assert!(
        !diff.contains("-line3"),
        "Insertion should not cascade: line3 should not appear as removed. Got:\n{}",
        diff
    );
    assert!(
        !diff.contains("-line4"),
        "Insertion should not cascade: line4 should not appear as removed. Got:\n{}",
        diff
    );
}

#[test]
fn test_unified_diff_deletion_does_not_cascade() {
    let before = "line1\nline2\nline3\nline4\nline5\n";
    let after = "line1\nline3\nline4\nline5\n";
    let diff = format_unified_diff(before, after, "test.rs");

    assert!(diff.contains("-line2"), "Should show line2 as removed");
    assert!(
        !diff.contains("-line3"),
        "Deletion should not cascade: line3 should not appear as removed. Got:\n{}",
        diff
    );
}
