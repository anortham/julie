// Inline tests extracted from extractors/regex/groups.rs
//
// These tests validate regex group extraction utilities, including:
// - Capturing group detection (basic groups vs non-capturing and named groups)
// - Named group name extraction ((?<name>...) and (?P<name>...) formats)
//
// Migration from inline tests to centralized test module:
// - Date extracted: 2025-10-16
// - Original location: src/extractors/regex/groups.rs (lines 23-40)
// - Tests extracted: 2
// - Original test module size: 18 lines
// - Original file size reduction: 41 lines → 22 lines (46% reduction)
// - Visibility changes: pub(super) → pub(crate) for test accessibility

use crate::extractors::regex::groups::{extract_group_name, is_capturing_group};

#[test]
fn test_is_capturing_group() {
    assert!(is_capturing_group("(abc)"));
    assert!(!is_capturing_group("(?:abc)"));
    assert!(!is_capturing_group("(?<name>abc)"));
}

#[test]
fn test_extract_group_name() {
    assert_eq!(extract_group_name("(?<name>...)"), Some("name".to_string()));
    assert_eq!(
        extract_group_name("(?P<name>...)"),
        Some("name".to_string())
    );
    assert_eq!(extract_group_name("(abc)"), None);
}
