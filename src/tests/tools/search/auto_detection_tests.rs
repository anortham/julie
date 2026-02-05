//! Tests for auto-detection of search method from query patterns
//!
//! With Tantivy as the sole search engine, detect_search_method always returns
//! "text". This test suite verifies that contract — all queries route to text search
//! regardless of query content.

use crate::tools::search::detect_search_method;

#[test]
fn test_all_queries_route_to_text() {
    // With Tantivy handling everything, all queries go to text search
    assert_eq!(detect_search_method(": BaseClass"), "text");
    assert_eq!(detect_search_method("ILogger<"), "text");
    assert_eq!(detect_search_method("[Fact]"), "text");
    assert_eq!(detect_search_method("getUserData()"), "text");
    assert_eq!(detect_search_method("if { }"), "text");
    assert_eq!(detect_search_method("=> user.id"), "text");
    assert_eq!(detect_search_method("user?.name"), "text");
    assert_eq!(detect_search_method("isValid && isActive"), "text");
}

#[test]
fn test_natural_language_routes_to_text() {
    // Natural language also goes to Tantivy text search
    assert_eq!(detect_search_method("authentication logic"), "text");
    assert_eq!(detect_search_method("user management code"), "text");
    assert_eq!(detect_search_method("find payment processing"), "text");
}

#[test]
fn test_simple_identifier_routes_to_text() {
    assert_eq!(detect_search_method("getUserData"), "text");
    assert_eq!(detect_search_method("PaymentService"), "text");
    assert_eq!(detect_search_method("handle_request"), "text");
}

#[test]
fn test_edge_cases_route_to_text() {
    assert_eq!(detect_search_method(""), "text");
    assert_eq!(detect_search_method("   "), "text");
}
