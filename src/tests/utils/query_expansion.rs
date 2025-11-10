//! Inline tests extracted from utils/query_expansion.rs
//!
//! These tests were originally inline in the query_expansion module but have been
//! extracted to a dedicated test file for better code organization and clarity.
//! All tests verify query expansion functionality for multi-word search queries.

use crate::utils::query_expansion::*;

#[test]
fn test_to_camelcase() {
    assert_eq!(to_camelcase("user service"), "UserService");
    assert_eq!(to_camelcase("get user data"), "GetUserData");
    assert_eq!(to_camelcase("handle request"), "HandleRequest");
    assert_eq!(to_camelcase("process payment info"), "ProcessPaymentInfo");
    assert_eq!(to_camelcase("single"), "Single");
    assert_eq!(to_camelcase(""), "");
}

#[test]
fn test_to_snake_case() {
    assert_eq!(to_snake_case("user service"), "user_service");
    assert_eq!(to_snake_case("get user data"), "get_user_data");
    assert_eq!(to_snake_case("handle request"), "handle_request");
    assert_eq!(to_snake_case("single"), "single");
    assert_eq!(to_snake_case(""), "");
}

#[test]
fn test_to_lowercase_camelcase() {
    assert_eq!(to_lowercase_camelcase("user service"), "userService");
    assert_eq!(to_lowercase_camelcase("get user data"), "getUserData");
    assert_eq!(to_lowercase_camelcase("handle request"), "handleRequest");
    assert_eq!(to_lowercase_camelcase("single"), "single");
}

#[test]
fn test_to_wildcard_query() {
    // FTS5 uses space-separated (implicit AND), not "AND" keyword
    assert_eq!(to_wildcard_query("user service"), "user* service*");
    assert_eq!(
        to_wildcard_query("get user data"),
        "get* user* data*"
    );
}

#[test]
fn test_to_or_query() {
    assert_eq!(to_or_query("user service"), "(user OR service)");
    assert_eq!(to_or_query("get user data"), "(get OR user OR data)");
}

#[test]
fn test_to_fuzzy_query() {
    assert_eq!(to_fuzzy_query("user service"), "user~1 service~1");
    assert_eq!(to_fuzzy_query("get user data"), "get~1 user~1 data~1");
}

#[test]
fn test_expand_query() {
    let variants = expand_query("user service");

    // Should contain original
    assert!(variants.contains(&"user service".to_string()));

    // Should contain CamelCase
    assert!(variants.contains(&"UserService".to_string()));

    // Should contain snake_case
    assert!(variants.contains(&"user_service".to_string()));

    // Should contain camelCase
    assert!(variants.contains(&"userService".to_string()));

    // Should generate multiple variants
    assert!(
        variants.len() >= 3,
        "Should generate at least 3 variants, got {}",
        variants.len()
    );
}

#[test]
fn test_expand_query_single_word() {
    let variants = expand_query("user");

    // Single words should still work but won't generate many variants
    assert!(variants.contains(&"user".to_string()));
}

#[test]
fn test_expand_query_generates_valid_fts5_syntax() {
    // Bug reproduction test: FTS5 does NOT support "AND" keyword
    // Correct syntax: space-separated terms (implicit AND)
    let variants = expand_query("plan tool implementation");

    // Should NOT contain "AND" keyword (invalid FTS5 syntax)
    for variant in &variants {
        assert!(
            !variant.contains(" AND "),
            "FTS5 query should not contain 'AND' keyword. Found in variant: '{}'",
            variant
        );
    }

    // Should contain space-separated implicit AND query
    assert!(
        variants.contains(&"plan tool implementation".to_string()),
        "Should contain implicit AND query (space-separated)"
    );

    // Should contain space-separated wildcard query
    assert!(
        variants.contains(&"plan* tool* implementation*".to_string()),
        "Should contain space-separated wildcard query"
    );
}
