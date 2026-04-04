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
