//! Query Preprocessor - Intelligent query analysis and routing for code search
//!
//! This module implements the preprocessing layer that validates and analyzes queries
//! before they reach the search engine (Tantivy).
//!
//! With Tantivy + CodeTokenizer:
//! - CamelCase/snake_case splitting happens at INDEX TIME (no query expansion needed)
//! - Query parsing is handled by Tantivy (no manual sanitization needed)
//! - Our job is just to: validate, detect type, and route appropriately
//!
//! Pipeline: Validate → Detect → Route
//!
//! Query Types:
//! - Symbol: Class/function/method names (UserService, getUserData, MAX_BUFFER_SIZE)
//! - Pattern: Code syntax patterns ([Test], async fn, =>, impl Trait)
//! - Glob: File path patterns (*.rs, **/Program.cs, src/**/*.ts)
//! - Standard: Natural language full-text search (error handling logic)

use anyhow::{Result, anyhow};
use regex::Regex;

/// Query types determine how we process and route searches
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    /// Exact symbol name matching - for finding classes, functions, methods
    /// Examples: UserService, getUserData, get_symbols, MAX_BUFFER_SIZE
    Symbol,

    /// Code syntax patterns - preserve special characters and operators
    /// Examples: [Test], async fn, =>, ::{}, impl Trait for
    Pattern,

    /// File path glob patterns - for finding files by path
    /// Examples: *.rs, **/Program.cs, src/**/*.ts
    Glob,

    /// Natural language full-text search - tokenized search
    /// Examples: "error handling logic", "async operations"
    Standard,
}

/// Preprocessed query ready for Tantivy execution
#[derive(Debug)]
pub struct PreprocessedQuery {
    pub original: String,
    pub query_type: QueryType,
}

/// Detects the optimal query type based on query characteristics
///
/// Detection Logic:
/// 1. File globs (*.ext, **/*) → Glob
/// 2. Code patterns (brackets, operators, keywords) → Pattern
/// 3. Simple identifiers (CamelCase, snake_case, CONSTANTS) → Symbol
/// 4. Everything else → Standard (natural language)
pub fn detect_query_type(query: &str) -> QueryType {
    let trimmed = query.trim();

    // Empty query defaults to standard
    if trimmed.is_empty() {
        return QueryType::Standard;
    }

    // Check for file glob patterns
    if is_glob_pattern(trimmed) {
        return QueryType::Glob;
    }

    // Check for code patterns (special characters, operators, keywords)
    if contains_code_pattern(trimmed) {
        return QueryType::Pattern;
    }

    // Check for simple symbol queries (identifiers)
    if is_simple_symbol(trimmed) {
        return QueryType::Symbol;
    }

    // Default to standard full-text search
    QueryType::Standard
}

/// Checks if query is a file glob pattern
fn is_glob_pattern(query: &str) -> bool {
    // Glob patterns contain file extensions or path wildcards
    // Examples: *.rs, **/*.ts, src/**/*.js, **/Program.cs

    // Has file extension with wildcard
    if query.contains("*.") {
        return true;
    }

    // Has double-star path wildcard
    if query.contains("**/") {
        return true;
    }

    // Ends with common file extensions (even without wildcards)
    let has_extension = query.ends_with(".rs")
        || query.ends_with(".ts")
        || query.ends_with(".js")
        || query.ends_with(".py")
        || query.ends_with(".java")
        || query.ends_with(".cs")
        || query.ends_with(".go")
        || query.ends_with(".php")
        || query.ends_with(".rb")
        || query.ends_with(".swift")
        || query.ends_with(".kt")
        || query.ends_with(".cpp")
        || query.ends_with(".c")
        || query.ends_with(".h")
        || query.ends_with(".hpp");

    if has_extension {
        return true;
    }

    false
}

/// Checks if query contains code syntax patterns
fn contains_code_pattern(query: &str) -> bool {
    // Special characters that indicate code patterns
    let special_chars = ['[', ']', '{', '}', '<', '>', '(', ')', ':', ';', '.', ','];

    for ch in special_chars {
        if query.contains(ch) {
            return true;
        }
    }

    // Two-character operators
    let operators = [
        "=>", "??", "?.", "::", "->", "+=", "-=", "*=", "/=", "==", "!=", ">=", "<=", "&&", "||",
        "<<", ">>",
    ];

    for op in operators {
        if query.contains(op) {
            return true;
        }
    }

    // Language keywords that indicate code patterns
    let keywords = [
        "async",
        "await",
        "class",
        "interface",
        "struct",
        "enum",
        "impl",
        "trait",
        "fn",
        "function",
        "def",
        "func",
        "method",
        "var",
        "let",
        "const",
        "public",
        "private",
    ];

    for keyword in keywords {
        // Check for keyword as whole word (with word boundaries)
        if query.split_whitespace().any(|word| word == keyword) {
            return true;
        }
    }

    false
}

/// Checks if query is a simple programming symbol/identifier
fn is_simple_symbol(query: &str) -> bool {
    // Must not contain spaces (multi-word queries are Standard)
    if query.contains(' ') {
        return false;
    }

    // Check if it matches valid identifier pattern: [A-Za-z_][A-Za-z0-9_]*
    let identifier_regex = Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap();
    if identifier_regex.is_match(query) {
        return true;
    }

    // Check for CamelCase pattern (indicates symbol name)
    // Examples: UserService, getUserData, HttpClient
    let camel_case_regex = Regex::new(r"[A-Z][a-z]+|[a-z]+[A-Z]").unwrap();
    if camel_case_regex.is_match(query) {
        return true;
    }

    false
}

/// Validates query and rejects invalid patterns early
pub fn validate_query(query: &str) -> Result<()> {
    let trimmed = query.trim();

    // Check for pure wildcards (meaningless queries)
    if is_pure_wildcard(trimmed) {
        return Err(anyhow!(
            "Invalid query: pure wildcard patterns (*, **, ?, ??) are not allowed. \
             Wildcards must be combined with search terms (e.g., 'User*' or 'get*Data')"
        ));
    }

    // More validations can be added here as needed

    Ok(())
}

/// Checks if query is a pure wildcard with no search terms
fn is_pure_wildcard(query: &str) -> bool {
    // Remove all wildcards and check if anything remains
    let without_wildcards = query.replace(['*', '?'], "");
    without_wildcards.trim().is_empty()
}

/// Sanitizes query by removing problematic leading wildcards
/// Note: With Tantivy, minimal sanitization is needed.
pub fn sanitize_query(query: &str) -> String {
    let mut result = query.trim().to_string();

    // Remove leading wildcards (these cause performance issues)
    while result.starts_with('*') || result.starts_with('?') {
        result = result[1..].trim().to_string();
    }

    result
}

/// Full preprocessing pipeline
///
/// With Tantivy + CodeTokenizer, the pipeline is simple:
/// 1. Validate the query (reject pure wildcards, etc.)
/// 2. Detect the query type (Symbol/Pattern/Glob/Standard)
/// 3. Pass the original query to Tantivy (which handles all parsing)
pub fn preprocess_query(query: &str) -> Result<PreprocessedQuery> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return Err(anyhow!("Empty query"));
    }

    // Validate query
    validate_query(trimmed)?;

    // Detect type
    let query_type = detect_query_type(trimmed);

    Ok(PreprocessedQuery {
        original: trimmed.to_string(),
        query_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_type_detection() {
        assert_eq!(detect_query_type("UserService"), QueryType::Symbol);
        assert_eq!(detect_query_type("[Test]"), QueryType::Pattern);
        assert_eq!(detect_query_type("*.rs"), QueryType::Glob);
        assert_eq!(detect_query_type("error handling"), QueryType::Standard);
    }

    #[test]
    fn test_wildcard_validation() {
        assert!(validate_query("*").is_err());
        assert!(validate_query("**").is_err());
        assert!(validate_query("User*").is_ok());
    }

    #[test]
    fn test_sanitization() {
        assert_eq!(sanitize_query("*something"), "something");
        assert_eq!(sanitize_query("User*"), "User*");
    }

    #[test]
    fn test_preprocess_query_symbol() {
        let result = preprocess_query("UserService").unwrap();
        assert_eq!(result.query_type, QueryType::Symbol);
        assert_eq!(result.original, "UserService");
    }

    #[test]
    fn test_preprocess_query_pattern() {
        let result = preprocess_query("class CmsService : ICmsService").unwrap();
        assert_eq!(result.query_type, QueryType::Pattern);
        assert_eq!(result.original, "class CmsService : ICmsService");
    }

    #[test]
    fn test_preprocess_query_glob() {
        let result = preprocess_query("*.rs").unwrap();
        assert_eq!(result.query_type, QueryType::Glob);
        assert_eq!(result.original, "*.rs");
    }

    #[test]
    fn test_preprocess_query_standard() {
        let result = preprocess_query("error handling logic").unwrap();
        assert_eq!(result.query_type, QueryType::Standard);
        assert_eq!(result.original, "error handling logic");
    }

    #[test]
    fn test_preprocess_query_rejects_pure_wildcard() {
        assert!(preprocess_query("*").is_err());
        assert!(preprocess_query("**").is_err());
    }
}
