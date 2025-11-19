//! Tests for auto-detection of search method from query patterns
//!
//! Tests the logic that automatically detects whether a query contains
//! code patterns (: < > [ ] ( ) { } => etc.) and should use pattern/exact search,
//! or is natural language and should use hybrid search.

use crate::tools::search::detect_search_method;

#[test]
fn test_detect_inheritance_pattern() {
    // Colon indicates inheritance or type annotation
    assert_eq!(
        detect_search_method(": BaseClass"),
        "text",
        "Colon should trigger text search for exact matching"
    );
    assert_eq!(detect_search_method("class: IUserService"), "text");
}

#[test]
fn test_detect_generic_pattern() {
    // Angle brackets indicate generics
    assert_eq!(
        detect_search_method("ILogger<"),
        "text",
        "Generic pattern should trigger text search"
    );
    assert_eq!(detect_search_method("Map<String, Vec<User>>"), "text");
    assert_eq!(detect_search_method("Result<T, E>"), "text");
}

#[test]
fn test_detect_array_bracket_pattern() {
    // Square brackets indicate arrays or attributes
    assert_eq!(
        detect_search_method("[Fact]"),
        "text",
        "Attribute pattern should trigger text search"
    );
    assert_eq!(detect_search_method("[Test]"), "text");
    assert_eq!(detect_search_method("array[index]"), "text");
}

#[test]
fn test_detect_function_call_pattern() {
    // Parentheses indicate function calls
    assert_eq!(
        detect_search_method("getUserData()"),
        "text",
        "Function call pattern should trigger text search"
    );
    assert_eq!(detect_search_method("fn process(data: &str)"), "text");
}

#[test]
fn test_detect_block_pattern() {
    // Curly braces indicate blocks
    assert_eq!(
        detect_search_method("if { }"),
        "text",
        "Block pattern should trigger text search"
    );
    assert_eq!(detect_search_method("struct User { }"), "text");
}

#[test]
fn test_detect_lambda_pattern() {
    // Lambda arrow operator
    assert_eq!(
        detect_search_method("=> user.id"),
        "text",
        "Lambda pattern should trigger text search"
    );
    assert_eq!(detect_search_method("map(x => x * 2)"), "text");
}

#[test]
fn test_detect_null_conditional_pattern() {
    // Null-conditional operator
    assert_eq!(
        detect_search_method("user?.name"),
        "text",
        "Null-conditional pattern should trigger text search"
    );
    assert_eq!(detect_search_method("data?.items?.first"), "text");
}

#[test]
fn test_detect_logical_and_pattern() {
    // Logical AND operator
    assert_eq!(
        detect_search_method("isValid && isActive"),
        "text",
        "Logical AND pattern should trigger text search"
    );
}

#[test]
fn test_natural_language_uses_hybrid() {
    // Natural language queries without code patterns should use hybrid
    assert_eq!(
        detect_search_method("authentication logic"),
        "hybrid",
        "Natural language should trigger hybrid search"
    );
    assert_eq!(detect_search_method("user management code"), "hybrid");
    assert_eq!(detect_search_method("find payment processing"), "hybrid");
    assert_eq!(
        detect_search_method("code that handles file uploads"),
        "hybrid"
    );
}

#[test]
fn test_simple_identifier_uses_hybrid() {
    // Simple identifiers without special chars should use hybrid
    assert_eq!(
        detect_search_method("getUserData"),
        "hybrid",
        "Simple identifier should use hybrid search"
    );
    assert_eq!(detect_search_method("PaymentService"), "hybrid");
    assert_eq!(detect_search_method("handle_request"), "hybrid");
}

#[test]
fn test_mixed_pattern_and_text() {
    // If query contains ANY code pattern char, use text search
    assert_eq!(
        detect_search_method("find ILogger<UserService> usage"),
        "text",
        "Mixed query with pattern should trigger text search"
    );
    assert_eq!(
        detect_search_method("classes that inherit: BaseService"),
        "text"
    );
}

#[test]
fn test_empty_query() {
    // Empty query should default to hybrid (will return no results anyway)
    assert_eq!(
        detect_search_method(""),
        "hybrid",
        "Empty query should default to hybrid"
    );
}

#[test]
fn test_whitespace_only() {
    // Whitespace-only should default to hybrid
    assert_eq!(
        detect_search_method("   "),
        "hybrid",
        "Whitespace-only should default to hybrid"
    );
}
