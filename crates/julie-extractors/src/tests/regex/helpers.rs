// Inline tests extracted from src/extractors/regex/helpers.rs
//
// These tests validate the helper functions used by the Regex extractor,
// including pattern validation, complexity calculation, and symbol kind determination.
//
// Migration from inline tests to centralized test module:
// - Date extracted: 2025-10-16
// - Original location: src/extractors/regex/helpers.rs (lines 190-215)
// - Tests extracted: 3
// - Original test module size: 26 lines

use crate::base::SymbolKind;
use crate::regex::helpers::{
    calculate_complexity, determine_pattern_kind, is_valid_regex_pattern,
};

#[test]
fn test_is_valid_regex_pattern() {
    assert!(is_valid_regex_pattern("\\d+"));
    assert!(is_valid_regex_pattern("[a-z]"));
    assert!(is_valid_regex_pattern("(?=test)"));
    assert!(!is_valid_regex_pattern(""));
}

#[test]
fn test_calculate_complexity() {
    assert_eq!(calculate_complexity("a"), 0);
    assert_eq!(calculate_complexity("a*"), 1);
    assert_eq!(calculate_complexity("[a-z]+"), 3);
}

#[test]
fn test_determine_pattern_kind() {
    assert_eq!(determine_pattern_kind("[abc]"), SymbolKind::Class);
    assert_eq!(determine_pattern_kind("a*"), SymbolKind::Function);
    assert_eq!(determine_pattern_kind("^"), SymbolKind::Constant);
}
