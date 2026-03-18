//! Unit tests for compute_line_changes function
//!
//! TDD: These tests were written BEFORE the implementation.

#[cfg(test)]
mod tests {
    use crate::tools::refactoring::compute_line_changes;

    #[test]
    fn test_compute_line_changes() {
        let old = "fn foo() {\n    let x = foo();\n    println!(\"hello\");\n}\n";
        let new = "fn bar() {\n    let x = bar();\n    println!(\"hello\");\n}\n";

        let changes = compute_line_changes(old, new);
        assert_eq!(changes.len(), 2, "should detect 2 changed lines");

        assert_eq!(changes[0].line_number, 1);
        assert!(
            changes[0].old_line.contains("foo"),
            "old_line[0] should contain 'foo', got: {}",
            changes[0].old_line
        );
        assert!(
            changes[0].new_line.contains("bar"),
            "new_line[0] should contain 'bar', got: {}",
            changes[0].new_line
        );

        assert_eq!(changes[1].line_number, 2);
        assert!(
            changes[1].old_line.contains("foo"),
            "old_line[1] should contain 'foo', got: {}",
            changes[1].old_line
        );
        assert!(
            changes[1].new_line.contains("bar"),
            "new_line[1] should contain 'bar', got: {}",
            changes[1].new_line
        );
    }

    #[test]
    fn test_compute_line_changes_no_changes() {
        let content = "fn foo() {}\n";
        let changes = compute_line_changes(content, content);
        assert!(
            changes.is_empty(),
            "identical content should produce no changes"
        );
    }

    #[test]
    fn test_compute_line_changes_single_line() {
        let old = "fn getUserData() {}";
        let new = "fn fetchUserData() {}";

        let changes = compute_line_changes(old, new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].line_number, 1);
        assert!(changes[0].old_line.contains("getUserData"));
        assert!(changes[0].new_line.contains("fetchUserData"));
    }

    #[test]
    fn test_compute_line_changes_preserves_unchanged_lines() {
        let old = "fn foo() {}\nfn bar() {}\nfn baz() {}\n";
        let new = "fn foo() {}\nfn bar() {}\nfn qux() {}\n";

        let changes = compute_line_changes(old, new);
        // Only line 3 changed
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].line_number, 3);
        assert!(changes[0].old_line.contains("baz"));
        assert!(changes[0].new_line.contains("qux"));
    }
}
