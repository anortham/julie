//! Query Preprocessor - Intelligent query analysis and routing for code search
//!
//! This module implements the preprocessing layer that ALL production search systems require.
//! It prevents the "bug of the day" loop by validating and transforming queries BEFORE
//! they reach FTS5, rather than patching crashes after the fact.
//!
//! Architecture inspired by coa-codesearch-mcp's SmartQueryPreprocessor which uses the same
//! pattern with Lucene.NET (proving this is necessary regardless of search engine).
//!
//! Pipeline: Validate → Detect → Route → Sanitize → Execute
//!
//! Query Types:
//! - Symbol: Class/function/method names (UserService, getUserData, MAX_BUFFER_SIZE)
//! - Pattern: Code syntax patterns ([Test], async fn, =>, impl Trait)
//! - Glob: File path patterns (*.rs, **/Program.cs, src/**/*.ts)
//! - Standard: Natural language full-text search (error handling logic)

use anyhow::{anyhow, Result};
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

/// Preprocessed query ready for FTS5 execution
#[derive(Debug)]
pub struct PreprocessedQuery {
    pub original_query: String,
    pub query_type: QueryType,
    pub fts5_query: String,
    pub search_field: String, // "name" for symbols, "content" for patterns/standard
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
pub fn sanitize_query(query: &str) -> String {
    let mut result = query.trim().to_string();

    // Remove leading wildcards (these cause performance issues)
    while result.starts_with('*') || result.starts_with('?') {
        result = result[1..].trim().to_string();
    }

    result
}

/// Sanitizes query for FTS5 based on query type
pub fn sanitize_for_fts5(query: &str, query_type: QueryType) -> String {
    match query_type {
        QueryType::Symbol => sanitize_symbol_for_fts5(query),
        QueryType::Pattern => sanitize_pattern_for_fts5(query),
        QueryType::Glob => sanitize_glob_for_fts5(query),
        QueryType::Standard => sanitize_standard_for_fts5(query),
    }
}

/// Sanitize symbol queries for FTS5
fn sanitize_symbol_for_fts5(query: &str) -> String {
    // CRITICAL FIX: Don't quote Symbol queries!
    //
    // The FTS5 tokenizer uses underscore as a separator:
    //   tokenize = "unicode61 separators '_::->.'"
    //
    // This means "handle_file_deleted" → ["handle", "file", "deleted"]
    //
    // If we wrap in quotes: "Deleted" → phrase search fails (no phrase "Deleted" exists)
    // Without quotes: Deleted → token match succeeds (token "deleted" exists)
    //
    // Token matching works for both snake_case and camelCase:
    // - "Deleted" matches "handle_file_deleted" (token "deleted")
    // - "getUserData" matches "getUserData" (exact match)
    // - "handle_file_change" matches "handle_file_change_static" (prefix match)
    //
    // This is the correct behavior for code search - find tokens, not exact phrases.
    query.to_string()
}

/// Sanitize pattern queries for FTS5
fn sanitize_pattern_for_fts5(query: &str) -> String {
    let mut result = query.to_string();

    // Remove regex operators that confuse FTS5
    // FTS5 treats these as special operators, not literal text

    // Strip backslash escapes first (regex escape sequences)
    result = result.replace('\\', "");

    // Handle regex patterns
    if result.contains(".*") {
        // "InputFile.*spreadsheet" → "InputFile spreadsheet"
        result = result.replace(".*", " ");
    }

    if result.contains(".+") {
        result = result.replace(".+", " ");
    }

    // Check if query contains code operators that should be preserved via quoting
    // IMPORTANT: Do this BEFORE removing . and ? to preserve operators like ?.
    let should_quote = result.contains('|')  // OR operator
        || result.contains('[') || result.contains(']')  // Brackets
        || result.contains('{') || result.contains('}')  // Braces
        || result.contains("=>")  // Arrow functions
        || result.contains("::")  // Scope resolution
        || result.contains("?."); // Optional chaining

    if should_quote {
        // Quote to preserve operators as literal search
        result = format!("\"{}\"", result.replace('"', ""));
    } else {
        // 🔥 FIX: Handle : (colon) specially - it's FTS5 column filter syntax
        // FTS5 treats : as column specification syntax (e.g., "name:term")
        // So "class Foo : Bar" is interpreted as "column Foo, term Bar" → syntax error
        // Split on : and convert to AND query for precise matching
        // Example: "class CmsService : ICmsService" → "class CmsService AND ICmsService"
        // This finds classes where BOTH names appear (implements/extends relationship)
        // BUT: Don't split :: (scope resolution) - that's already handled by should_quote above
        if result.contains(':') && !result.contains("::") {
            let parts: Vec<&str> = result.split(':').filter(|s| !s.is_empty()).collect();
            if parts.len() > 1 {
                result = parts.join(" AND ");
            }
        }

        // Only remove dots and question marks if NOT quoting (no code operators)
        // Remove standalone dots (FTS5 column separator / operator)
        // "string.method" → "string method"
        result = result.replace('.', " ");

        // Remove question marks (FTS5 wildcard operator, C# nullable syntax)
        // "string?" → "string", "List?" → "List"
        result = result.replace('?', "");
    }

    // Remove regex anchors (always)
    if result.contains('$') {
        // "end$" → "end"
        result = result.replace('$', "");
    }

    if result.contains('^') && !result.contains("^^") {
        // "^start" → "start" (but preserve ^^ which might be XOR)
        result = result.replace('^', "");
    }

    result
}

/// Sanitize glob queries for FTS5
fn sanitize_glob_for_fts5(query: &str) -> String {
    // Glob queries like "*.razor" need sanitization for FTS5
    // Remove glob operators (*, ?, **) and FTS5 special chars (.)
    // This prevents "fts5: syntax error near '.'" errors
    query
        .replace("**", "")
        .replace("*", "")
        .replace("?", "")
        .replace(".", "") // FTS5 treats . as an operator
        .trim()
        .to_string()
}

/// Sanitize standard queries for FTS5
fn sanitize_standard_for_fts5(query: &str) -> String {
    // Standard queries use FTS5's natural tokenization
    // Just ensure we don't have problematic characters
    let mut result = query.to_string();

    // Handle hyphens FIRST - FTS5 treats "-" as subtraction operator
    // "tree-sitter" → "tree OR sitter" to match tokenized content
    // Don't split negative numbers like "-42" or ranges like "1-10"
    if result.contains('-') && !result.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '.') {
        let parts: Vec<&str> = result.split('-').filter(|s| !s.is_empty()).collect();
        if parts.len() > 1 {
            result = parts.join(" OR ");
        }
    }

    // Remove regex operators if present
    result = result.replace(".*", " ");
    result = result.replace(".+", " ");
    result = result.replace('$', "");

    // Remove standalone dots and question marks (FTS5 operators)
    // IMPORTANT: Do this AFTER handling .* and .+ patterns
    result = result.replace('.', " ");
    result = result.replace('?', "");

    result.trim().to_string()
}

/// Process query based on type (strip noise words, etc.)
pub fn process_query(query: &str, query_type: QueryType) -> String {
    match query_type {
        QueryType::Symbol => process_symbol_query(query),
        QueryType::Pattern => process_pattern_query(query),
        QueryType::Glob => process_glob_query(query),
        QueryType::Standard => process_standard_query(query),
    }
}

/// Process symbol queries - remove language keywords
fn process_symbol_query(query: &str) -> String {
    let mut result = query.to_string();

    // Remove noise words (language keywords)
    let noise_words = [
        "class",
        "interface",
        "struct",
        "enum",
        "function",
        "fn",
        "def",
        "method",
    ];

    for word in noise_words {
        // Remove keyword with space after it
        result = result.replace(&format!("{} ", word), "");
    }

    result.trim().to_string()
}

/// Process pattern queries - preserve as-is
fn process_pattern_query(query: &str) -> String {
    // Pattern queries need special characters preserved
    query.to_string()
}

/// Process glob queries - preserve as-is
fn process_glob_query(query: &str) -> String {
    // Glob queries are file paths
    query.to_string()
}

/// Process standard queries - convert multi-word queries to FTS5 AND logic
fn process_standard_query(query: &str) -> String {
    let trimmed = query.trim();

    // For single-word queries, return as-is
    if !trimmed.contains(' ') {
        return trimmed.to_string();
    }

    // For multi-word queries, use FTS5 AND logic: "a b c" → "a AND b AND c"
    // This finds documents containing ALL terms (Google-style search)
    // FTS5's tokenizer will handle CamelCase/snake_case splitting automatically
    // Example: "getUserData service" → "getUserData AND service"
    //   FTS5 tokenizes "getUserData" → ["get", "User", "Data"] at index time
    //   Query matches documents containing both "getUserData" AND "service"
    let and_query = trimmed
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" AND ");

    and_query
}

/// Full preprocessing pipeline
pub fn preprocess_query(query: &str) -> Result<PreprocessedQuery> {
    // Step 1: Validate
    validate_query(query)?;

    // Step 2: Detect type
    let query_type = detect_query_type(query);

    // Step 3: Sanitize wildcards
    let sanitized = sanitize_query(query);

    // Step 4: Process based on type
    let processed = process_query(&sanitized, query_type);

    // Step 5: Sanitize for FTS5
    let fts5_query = sanitize_for_fts5(&processed, query_type);

    // Step 6: Determine search field
    let search_field = match query_type {
        QueryType::Symbol => "name".to_string(), // Search symbol names
        QueryType::Pattern => "content".to_string(), // Search file content
        QueryType::Standard => "content".to_string(), // Search file content
        QueryType::Glob => "file_path".to_string(), // Search file paths
    };

    Ok(PreprocessedQuery {
        original_query: query.to_string(),
        query_type,
        fts5_query,
        search_field,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_type_detection_integration() {
        assert_eq!(detect_query_type("UserService"), QueryType::Symbol);
        assert_eq!(detect_query_type("[Test]"), QueryType::Pattern);
        assert_eq!(detect_query_type("*.rs"), QueryType::Glob);
        assert_eq!(detect_query_type("error handling"), QueryType::Standard);
    }

    #[test]
    fn test_wildcard_validation_integration() {
        assert!(validate_query("*").is_err());
        assert!(validate_query("**").is_err());
        assert!(validate_query("User*").is_ok());
    }

    #[test]
    fn test_sanitization_integration() {
        assert_eq!(sanitize_query("*something"), "something");
        assert_eq!(sanitize_query("User*"), "User*");
    }

    #[test]
    fn test_colon_handling_in_patterns() {
        // Bug reproduction: "class CmsService : ICmsService" → Pattern query → FTS5 syntax error
        // FTS5 treats single : as column filter syntax (like "column:term")

        // Question: Should this be AND or OR?
        // - OR: Matches if EITHER name appears (broad, finds more results)
        // - AND: Matches if BOTH names appear (precise, finds exact inheritance)
        //
        // Decision: Use AND for precise matching of inheritance/implements relationships
        // User searching "class Foo : Bar" wants to find where Foo implements Bar,
        // which means BOTH names must be present in the symbol definition.

        // Single colon (inheritance/implements syntax)
        let result = preprocess_query("class CmsService : ICmsService").unwrap();
        assert_eq!(result.query_type, QueryType::Pattern);
        // Should NOT contain bare colon - should be quoted or converted to AND/OR
        assert!(!result.fts5_query.contains(" : ") &&
                (result.fts5_query.contains(" AND ") || result.fts5_query.contains(" OR ") || result.fts5_query.contains("\":")));

        // Double colon (scope resolution) - already handled
        let result = preprocess_query("std::vector").unwrap();
        assert_eq!(result.query_type, QueryType::Pattern);
        // Should be quoted to preserve ::
        assert!(result.fts5_query.contains("::") || result.fts5_query.contains("\""));
    }
}
