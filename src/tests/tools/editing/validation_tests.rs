//! Tests for bracket balance validation and diff formatting.

use crate::tools::editing::validation::{check_bracket_balance, format_unified_diff, should_check_balance};

#[test]
fn test_balanced_edit_no_warning() {
    let before = "fn main() {\n    println!(\"hello\");\n}\n";
    let after = "fn main() {\n    println!(\"world\");\n}\n";
    assert!(check_bracket_balance(before, after).is_none());
}

#[test]
fn test_edit_removing_close_brace_warns() {
    let before = "fn main() {\n    let x = 1;\n}\n";
    let after = "fn main() {\n    let x = 1;\n";
    assert!(check_bracket_balance(before, after).is_some());
}

#[test]
fn test_edit_adding_extra_close_paren_warns() {
    let before = "fn main() {\n    foo();\n}\n";
    let after = "fn main() {\n    foo());\n}\n";
    assert!(check_bracket_balance(before, after).is_some());
}

#[test]
fn test_empty_to_empty_no_warning() {
    assert!(check_bracket_balance("", "").is_none());
}

#[test]
fn test_string_literals_with_brackets_preserved() {
    // Both before and after have "unbalanced" brackets inside string literals.
    // The delta is zero, so no warning should be emitted.
    let before = "let s = \"foo())\";\nlet t = \"bar{\";\n";
    let after = "let s = \"baz())\";\nlet t = \"bar{\";\n";
    assert!(
        check_bracket_balance(before, after).is_none(),
        "Edit preserving bracket balance in string literals should produce no warning"
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
fn test_unified_diff_has_hunk_headers() {
    // 5-line file, change at line 3. Entire file within context range = 1 hunk.
    let before = "line1\nline2\nline3\nline4\nline5\n";
    let after = "line1\nline2\nchanged\nline4\nline5\n";
    let diff = format_unified_diff(before, after, "test.rs");
    assert!(
        diff.contains("@@ -1,5 +1,5 @@"),
        "Hunk header should show correct line numbers. Got:\n{}",
        diff
    );
}

#[test]
fn test_unified_diff_multiple_hunks_separated() {
    // 20 lines, changes at line 3 and line 18 (far enough apart for 2 hunks)
    let mut before_lines = Vec::new();
    let mut after_lines = Vec::new();
    for i in 1..=20 {
        before_lines.push(format!("line{}", i));
        if i == 3 {
            after_lines.push("changed3".to_string());
        } else if i == 18 {
            after_lines.push("changed18".to_string());
        } else {
            after_lines.push(format!("line{}", i));
        }
    }
    let before = before_lines.join("\n") + "\n";
    let after = after_lines.join("\n") + "\n";
    let diff = format_unified_diff(&before, &after, "test.rs");

    // Each hunk header has @@ at start and end, so 2 hunks = 4 @@ markers
    let at_count = diff.matches("@@").count();
    assert_eq!(
        at_count, 4,
        "Should have 2 hunk headers (4 @@ markers). Got:\n{}",
        diff
    );
}

#[test]
fn test_unified_diff_insertion_line_counts() {
    // Insert a line: old has 5 lines, new has 6
    let before = "line1\nline2\nline3\nline4\nline5\n";
    let after = "line1\nline2\nINSERTED\nline3\nline4\nline5\n";
    let diff = format_unified_diff(before, after, "test.rs");
    // Entire file within context, insertion adds 1 new-side line
    assert!(
        diff.contains("@@ -1,5 +1,6 @@"),
        "Insertion hunk should show old=5, new=6. Got:\n{}",
        diff
    );
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
