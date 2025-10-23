// Query Preprocessor Tests - COMPREHENSIVE AGENT QUERY COVERAGE
//
// This test suite captures the ACTUAL queries that AI agents run against Julie.
// Every test case here represents a real-world scenario that must work reliably.
//
// Test Organization:
// 1. Symbol Searches - Finding classes, functions, methods by name
// 2. Pattern Searches - Code syntax patterns (attributes, operators, keywords)
// 3. File Glob Searches - Finding files by path patterns
// 4. Invalid Queries - Queries that should be rejected with clear errors
// 5. Edge Cases - Boundary conditions and corner cases
//
// TDD Approach: All tests written FIRST, then implementation follows.

use crate::tools::search::{detect_query_type, preprocess_query, validate_query,
                           sanitize_query, sanitize_for_fts5, process_query,
                           QueryType};

// ============================================================================
// Query Type Detection Tests
// ============================================================================
// These tests verify that we correctly identify what kind of search the user wants.

#[cfg(test)]
mod query_type_detection {
    use super::*;

    #[test]
    fn test_detect_simple_symbol_query() {
        // When I search for "UserService", I want to find the class
        let query = "UserService";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Symbol);
    }

    #[test]
    fn test_detect_camelcase_symbol_query() {
        // When I search for "getUserData", I want to find the method
        let query = "getUserData";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Symbol);
    }

    #[test]
    fn test_detect_snake_case_symbol_query() {
        // When I search for "get_symbols", I want to find the function
        let query = "get_symbols";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Symbol);
    }

    #[test]
    fn test_detect_constant_symbol_query() {
        // When I search for "MAX_BUFFER_SIZE", I want to find the constant
        let query = "MAX_BUFFER_SIZE";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Symbol);
    }

    #[test]
    fn test_detect_pattern_with_brackets() {
        // When I search for "[Test]", I want to find test attributes
        let query = "[Test]";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Pattern);
    }

    #[test]
    fn test_detect_pattern_with_code_syntax() {
        // When I search for "async fn", I want to find async functions
        let query = "async fn";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Pattern);
    }

    #[test]
    fn test_detect_operator_query() {
        // When I search for "=>", I want to find lambda/arrow functions
        let query = "=>";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Pattern);
    }

    #[test]
    fn test_detect_glob_pattern() {
        // When I search for "*.rs", I want to find Rust files
        let query = "*.rs";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Glob);
    }

    #[test]
    fn test_detect_nested_glob_pattern() {
        // When I search for "**/Program.cs", I want to find nested files
        let query = "**/Program.cs";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Glob);
    }

    #[test]
    fn test_detect_standard_text_query() {
        // When I search for "error handling logic", it's natural language
        let query = "error handling logic";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Standard);
    }

    #[test]
    fn test_detect_simple_filename_as_glob() {
        // When I search for "Program.cs", I want to find that FILE
        // This should be detected as Glob, not Symbol
        let query = "Program.cs";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Glob);
    }

    #[test]
    fn test_detect_filename_with_path() {
        // When I search for "src/main.rs", I want to find that FILE
        // This is detected as Glob because it ends with .rs extension
        // The '/' doesn't prevent glob detection - file paths are globs!
        let query = "src/main.rs";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Glob); // Ends with .rs, so glob
    }
}

// ============================================================================
// Wildcard Validation Tests
// ============================================================================
// These tests verify that we reject invalid wildcards BEFORE execution.

#[cfg(test)]
mod wildcard_validation {
    use super::*;

    #[test]
    fn test_reject_pure_wildcard_star() {
        // Pure "*" should be rejected - it's meaningless
        let query = "*";
        let result = validate_query(query);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("pure wildcard"));
    }

    #[test]
    fn test_reject_pure_wildcard_double_star() {
        // Pure "**" should be rejected - it's meaningless
        let query = "**";
        let result = validate_query(query);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("pure wildcard"));
    }

    #[test]
    fn test_reject_pure_wildcard_triple_star() {
        // Pure "***" should be rejected - it's meaningless
        let query = "***";
        let result = validate_query(query);
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_pure_wildcard_question() {
        // Pure "?" should be rejected - it's meaningless
        let query = "?";
        let result = validate_query(query);
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_leading_wildcard() {
        // "*something" should be sanitized to "something"
        let query = "*something";
        let result = sanitize_query(query);
        assert_eq!(result, "something");
    }

    #[test]
    fn test_allow_trailing_wildcard() {
        // "User*" is valid - finds UserService, UserRepository, etc.
        let query = "User*";
        let result = validate_query(query);
        assert!(result.is_ok());
    }

    #[test]
    fn test_allow_embedded_wildcard() {
        // "get*Data" is valid - finds getUserData, getCustomerData, etc.
        let query = "get*Data";
        let result = validate_query(query);
        assert!(result.is_ok());
    }

    #[test]
    fn test_allow_glob_wildcards() {
        // "**/*.rs" is valid for file path matching
        let query = "**/*.rs";
        let result = validate_query(query);
        assert!(result.is_ok());
    }
}

// ============================================================================
// FTS5 Sanitization Tests
// ============================================================================
// These tests verify that we generate valid FTS5 queries.

#[cfg(test)]
mod fts5_sanitization {
    use super::*;

    #[test]
    fn test_sanitize_regex_pattern_with_dot_star() {
        // "InputFile.*spreadsheet" should NOT crash FTS5
        // This was the bug from TODO.md - FTS5 doesn't support regex
        let query = "InputFile.*spreadsheet";
        let sanitized = sanitize_for_fts5(query, QueryType::Pattern);

        // Should be converted to phrase search or sanitized
        assert!(!sanitized.contains(".*")); // No regex operators
    }

    #[test]
    fn test_sanitize_regex_end_anchor() {
        // "end$" should be sanitized - FTS5 treats $ as operator
        let query = "end$";
        let sanitized = sanitize_for_fts5(query, QueryType::Pattern);

        assert!(!sanitized.contains("$"));
    }

    #[test]
    fn test_sanitize_regex_alternation() {
        // "foo|bar" should be sanitized - FTS5 treats | as OR operator
        let query = "foo|bar";
        let sanitized = sanitize_for_fts5(query, QueryType::Pattern);

        // Should be quoted or | should be escaped
        assert!(sanitized.contains("\"") || !sanitized.contains("|"));
    }

    #[test]
    fn test_sanitize_regex_escaped_dot() {
        // "file\.txt" should be sanitized - backslash escapes confuse FTS5
        let query = r"file\.txt";
        let sanitized = sanitize_for_fts5(query, QueryType::Pattern);

        assert!(!sanitized.contains("\\")); // No backslash escapes
    }

    #[test]
    fn test_preserve_code_operators() {
        // "=>" should be preserved for code search
        let query = "=>";
        let sanitized = sanitize_for_fts5(query, QueryType::Pattern);

        // Should be quoted to preserve as phrase
        assert!(sanitized.contains("=>"));
    }

    #[test]
    fn test_preserve_scope_resolution() {
        // "::" should be preserved for Rust/C++ searches
        let query = "std::vector";
        let sanitized = sanitize_for_fts5(query, QueryType::Pattern);

        assert!(sanitized.contains("::"));
    }

    #[test]
    fn test_preserve_optional_chaining() {
        // "?." should be preserved for TypeScript searches
        let query = "user?.name";
        let sanitized = sanitize_for_fts5(query, QueryType::Pattern);

        assert!(sanitized.contains("?."));
    }
}

// ============================================================================
// Symbol Query Processing Tests
// ============================================================================
// Symbol searches should generate exact match queries.

#[cfg(test)]
mod symbol_query_processing {
    use super::*;

    #[test]
    fn test_process_simple_symbol() {
        // "UserService" → exact symbol match
        let query = "UserService";
        let processed = process_query(query, QueryType::Symbol);

        // Should search symbol names exactly
        assert!(processed.contains("UserService"));
    }

    #[test]
    fn test_process_symbol_with_noise_words() {
        // "class UserService" → strip "class", keep "UserService"
        let query = "class UserService";
        let processed = process_query(query, QueryType::Symbol);

        assert!(!processed.contains("class"));
        assert!(processed.contains("UserService"));
    }

    #[test]
    fn test_process_interface_keyword() {
        // "interface IRepository" → strip "interface"
        let query = "interface IRepository";
        let processed = process_query(query, QueryType::Symbol);

        assert!(!processed.contains("interface"));
        assert!(processed.contains("IRepository"));
    }

    #[test]
    fn test_process_function_keyword() {
        // "function getUserData" → strip "function"
        let query = "function getUserData";
        let processed = process_query(query, QueryType::Symbol);

        assert!(!processed.contains("function"));
        assert!(processed.contains("getUserData"));
    }
}

// ============================================================================
// Pattern Query Processing Tests
// ============================================================================
// Pattern searches should preserve syntax and special characters.

#[cfg(test)]
mod pattern_query_processing {
    use super::*;

    #[test]
    fn test_process_attribute_pattern() {
        // "[Test]" → preserve brackets for attribute search
        let query = "[Test]";
        let processed = process_query(query, QueryType::Pattern);

        assert!(processed.contains("[Test]") || processed.contains("\"[Test]\""));
    }

    #[test]
    fn test_process_async_keyword_pattern() {
        // "async fn" → preserve both words for syntax search
        let query = "async fn";
        let processed = process_query(query, QueryType::Pattern);

        assert!(processed.contains("async"));
        assert!(processed.contains("fn"));
    }

    #[test]
    fn test_process_impl_trait_pattern() {
        // "impl Trait for" → preserve syntax structure
        let query = "impl Trait for";
        let processed = process_query(query, QueryType::Pattern);

        assert!(processed.contains("impl"));
        assert!(processed.contains("Trait"));
        assert!(processed.contains("for"));
    }

    #[test]
    fn test_process_lambda_operator() {
        // "=> {" → preserve operator and brace
        let query = "=> {";
        let processed = process_query(query, QueryType::Pattern);

        assert!(processed.contains("=>"));
        assert!(processed.contains("{"));
    }
}

// ============================================================================
// Standard Query Processing Tests
// ============================================================================
// Standard searches are full-text search with AND logic.

#[cfg(test)]
mod standard_query_processing {
    use super::*;

    #[test]
    fn test_process_multi_word_query() {
        // "error handling logic" → all words must match
        let query = "error handling logic";
        let processed = process_query(query, QueryType::Standard);

        assert!(processed.contains("error"));
        assert!(processed.contains("handling"));
        assert!(processed.contains("logic"));
    }

    #[test]
    fn test_process_phrase_query() {
        // Quoted phrases should be preserved
        let query = "\"exact phrase\"";
        let processed = process_query(query, QueryType::Standard);

        assert!(processed.contains("\"exact phrase\""));
    }
}

// ============================================================================
// Integration Tests - End-to-End
// ============================================================================
// These tests verify the entire pipeline works together.

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_end_to_end_symbol_search() {
        // Complete pipeline: UserService → detect → validate → process → FTS5
        let query = "UserService";
        let result = preprocess_query(query);

        assert!(result.is_ok());
        let preprocessed = result.unwrap();
        assert_eq!(preprocessed.query_type, QueryType::Symbol);
        assert!(preprocessed.fts5_query.contains("UserService"));
    }

    #[test]
    fn test_end_to_end_invalid_query_rejection() {
        // Complete pipeline: * → detect → validate → REJECT
        let query = "*";
        let result = preprocess_query(query);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("pure wildcard"));
    }

    #[test]
    fn test_end_to_end_pattern_sanitization() {
        // Complete pipeline: InputFile.*spreadsheet → detect → sanitize → FTS5
        let query = "InputFile.*spreadsheet";
        let result = preprocess_query(query);

        assert!(result.is_ok());
        let preprocessed = result.unwrap();
        // Should not contain regex operators
        assert!(!preprocessed.fts5_query.contains(".*"));
    }

    #[test]
    fn test_glob_pattern_dot_sanitization() {
        // BUG REPRODUCTION: *.razor causes "fts5: syntax error near ".""
        // The dot in file extension globs MUST be removed for FTS5
        let query = "*.razor";
        let result = preprocess_query(query);

        assert!(result.is_ok(), "Glob query should preprocess successfully");
        let preprocessed = result.unwrap();
        assert_eq!(preprocessed.query_type, QueryType::Glob);

        // CRITICAL: FTS5 query must NOT contain dots (FTS5 treats . as operator)
        assert!(
            !preprocessed.fts5_query.contains("."),
            "FTS5 query should not contain dots: got '{}'",
            preprocessed.fts5_query
        );

        // Should be just "razor" or empty after sanitization
        assert!(
            preprocessed.fts5_query == "razor" || preprocessed.fts5_query.is_empty(),
            "Expected 'razor' or empty, got '{}'",
            preprocessed.fts5_query
        );
    }

    #[test]
    fn test_csharp_nullable_type_sanitization() {
        // BUG REPRODUCTION: string? causes "fts5: syntax error near '?'"
        // C# nullable types use ? which FTS5 treats as a special operator
        let query = "string?";
        let result = preprocess_query(query);

        assert!(result.is_ok(), "C# nullable query should preprocess successfully");
        let preprocessed = result.unwrap();

        // Query type can be Standard or Pattern - doesn't matter for sanitization
        // The key is that ? must be removed to prevent FTS5 errors

        // CRITICAL: FTS5 query must NOT contain ? (FTS5 treats it as wildcard/operator)
        assert!(
            !preprocessed.fts5_query.contains("?"),
            "FTS5 query should not contain '?': got '{}'",
            preprocessed.fts5_query
        );

        // Should be just "string" after sanitization
        assert_eq!(preprocessed.fts5_query, "string");
    }
}

// ============================================================================
// Snake Case Tokenization Bug Tests (FTS5 Separator Mismatch)
// ============================================================================
// CRITICAL BUG: FTS5 tokenizer uses underscore as separator, but Symbol queries
// wrap in quotes for "exact matching". This creates a mismatch:
//
// - "handle_file_deleted" is tokenized as ["handle", "file", "deleted"]
// - Query "Deleted" wrapped in quotes: "Deleted"
// - Phrase search "Deleted" looks for exact phrase, not token
// - Result: ZERO MATCHES even though data exists!
//
// Root Cause: tokenize = "unicode61 separators '_::->.'"
// Fix: Don't wrap Symbol queries in quotes - use token matching instead

#[cfg(test)]
mod snake_case_tokenization_bug {
    use super::*;

    #[test]
    fn test_search_deleted_should_find_handle_file_deleted() {
        // BUG REPRODUCTION: Searching "Deleted" should find "handle_file_deleted"
        // The tokenizer splits on underscore, so "deleted" is a token
        // But phrase search "Deleted" doesn't match token "deleted"
        let query = "Deleted";
        let result = preprocess_query(query);

        assert!(result.is_ok());
        let preprocessed = result.unwrap();
        assert_eq!(preprocessed.query_type, QueryType::Symbol);

        // CRITICAL: Should NOT wrap in quotes (phrase search fails with underscore tokenization)
        // Expected: Deleted (token match)
        // Wrong: "Deleted" (phrase search)
        assert!(
            !preprocessed.fts5_query.starts_with("\""),
            "Symbol query should NOT be wrapped in quotes for snake_case compatibility. Got: {}",
            preprocessed.fts5_query
        );
    }

    #[test]
    fn test_search_snake_case_function_name() {
        // BUG REPRODUCTION: Searching "handle_file_change_static" should find itself
        // With phrase search, this fails because underscores are separators
        let query = "handle_file_change_static";
        let result = preprocess_query(query);

        assert!(result.is_ok());
        let preprocessed = result.unwrap();
        assert_eq!(preprocessed.query_type, QueryType::Symbol);

        // Should NOT wrap in quotes - breaks snake_case token matching
        assert!(
            !preprocessed.fts5_query.starts_with("\""),
            "Snake_case symbol should use token matching, not phrase search. Got: {}",
            preprocessed.fts5_query
        );
    }

    #[test]
    fn test_search_scope_resolution_with_separator() {
        // BUG REPRODUCTION: "FileChangeType::Deleted" has :: separator
        // Both underscore AND :: are separators in tokenizer
        let query = "FileChangeType::Deleted";
        let result = preprocess_query(query);

        assert!(result.is_ok());
        let preprocessed = result.unwrap();
        // Will be detected as Pattern (contains ::)
        assert_eq!(preprocessed.query_type, QueryType::Pattern);

        // For patterns with code operators, quoting is acceptable
        // But we need to ensure tokens still match
        // This test documents the current behavior
    }

    #[test]
    fn test_search_partial_snake_case_token() {
        // Searching for a partial token should work via FTS5 prefix matching
        // "file_deleted" should find "handle_file_deleted"
        let query = "file_deleted";
        let result = preprocess_query(query);

        assert!(result.is_ok());
        let preprocessed = result.unwrap();

        // Should use token matching (no quotes) to find partial matches
        assert!(
            !preprocessed.fts5_query.starts_with("\""),
            "Partial snake_case should use token matching. Got: {}",
            preprocessed.fts5_query
        );
    }

    #[test]
    fn test_camelcase_doesnt_need_token_workaround() {
        // CamelCase doesn't have separators, so phrase search would work
        // But for consistency, we should use token matching for ALL symbols
        let query = "getUserData";
        let result = preprocess_query(query);

        assert!(result.is_ok());
        let preprocessed = result.unwrap();
        assert_eq!(preprocessed.query_type, QueryType::Symbol);

        // For consistency, don't quote ANY symbol queries
        assert!(
            !preprocessed.fts5_query.starts_with("\""),
            "Symbol queries should use token matching consistently. Got: {}",
            preprocessed.fts5_query
        );
    }
}

// All functions are now implemented in src/tools/search/query_preprocessor.rs
// and imported at the top of this file
