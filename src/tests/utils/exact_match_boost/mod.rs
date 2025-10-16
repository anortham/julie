// Inline tests extracted from utils/exact_match_boost.rs
//
// This module contains the complete test suite originally defined inline in the
// ExactMatchBoost implementation. Tests were extracted to reduce clutter in the
// main module and follow the test organization standards.
//
// Test count: 2 tests
// Original lines in utils/exact_match_boost.rs: ~40 lines (228-267)
//
// Tests verify:
// - Symbol tokenization (camelCase, snake_case, kebab-case, acronyms)
// - Query tokenization (whitespace normalization)

use crate::utils::exact_match_boost::ExactMatchBoost;

#[test]
fn test_tokenize_symbol() {
    assert_eq!(
        ExactMatchBoost::tokenize_symbol("getUserData"),
        vec!["get", "user", "data"]
    );

    assert_eq!(
        ExactMatchBoost::tokenize_symbol("user_service_impl"),
        vec!["user", "service", "impl"]
    );

    assert_eq!(
        ExactMatchBoost::tokenize_symbol("get-user-name"),
        vec!["get", "user", "name"]
    );

    assert_eq!(
        ExactMatchBoost::tokenize_symbol("XMLParser"),
        vec!["xml", "parser"]
    );
}

#[test]
fn test_tokenize_query() {
    assert_eq!(
        ExactMatchBoost::tokenize_query("get user data"),
        vec!["get", "user", "data"]
    );

    assert_eq!(
        ExactMatchBoost::tokenize_query("  hello   world  "),
        vec!["hello", "world"]
    );
}
