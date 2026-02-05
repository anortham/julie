// Query Preprocessor Tests - Simplified for Tantivy
//
// With Tantivy + CodeTokenizer, the preprocessor's job is simple:
// 1. Validate the query (reject pure wildcards)
// 2. Detect the query type (Symbol/Pattern/Glob/Standard)
// 3. Pass the original query to Tantivy
//
// Manual sanitization and query expansion are no longer needed
// since Tantivy handles query parsing and CodeTokenizer handles
// CamelCase/snake_case splitting at index time.

use crate::tools::search::{
    QueryType, detect_query_type, preprocess_query,
    sanitize_query, validate_query,
};

// ============================================================================
// Query Type Detection Tests
// ============================================================================

#[cfg(test)]
mod query_type_detection {
    use super::*;

    #[test]
    fn test_detect_simple_symbol_query() {
        let query = "UserService";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Symbol);
    }

    #[test]
    fn test_detect_camelcase_symbol_query() {
        let query = "getUserData";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Symbol);
    }

    #[test]
    fn test_detect_snake_case_symbol_query() {
        let query = "get_symbols";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Symbol);
    }

    #[test]
    fn test_detect_constant_symbol_query() {
        let query = "MAX_BUFFER_SIZE";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Symbol);
    }

    #[test]
    fn test_detect_pattern_with_brackets() {
        let query = "[Test]";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Pattern);
    }

    #[test]
    fn test_detect_pattern_with_code_syntax() {
        let query = "async fn";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Pattern);
    }

    #[test]
    fn test_detect_operator_query() {
        let query = "=>";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Pattern);
    }

    #[test]
    fn test_detect_glob_pattern() {
        let query = "*.rs";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Glob);
    }

    #[test]
    fn test_detect_nested_glob_pattern() {
        let query = "**/Program.cs";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Glob);
    }

    #[test]
    fn test_detect_standard_natural_language() {
        let query = "error handling logic";
        let query_type = detect_query_type(query);
        assert_eq!(query_type, QueryType::Standard);
    }
}

// ============================================================================
// Query Validation Tests
// ============================================================================

#[cfg(test)]
mod query_validation {
    use super::*;

    #[test]
    fn test_reject_pure_wildcard() {
        assert!(validate_query("*").is_err());
        assert!(validate_query("**").is_err());
        assert!(validate_query("?").is_err());
    }

    #[test]
    fn test_accept_wildcard_with_search_term() {
        assert!(validate_query("User*").is_ok());
        assert!(validate_query("*Service").is_ok());
        assert!(validate_query("get*Data").is_ok());
    }

    #[test]
    fn test_accept_normal_queries() {
        assert!(validate_query("UserService").is_ok());
        assert!(validate_query("*.rs").is_ok());
        assert!(validate_query("error handling").is_ok());
    }
}

// ============================================================================
// Query Sanitization Tests
// ============================================================================

#[cfg(test)]
mod query_sanitization {
    use super::*;

    #[test]
    fn test_remove_leading_wildcards() {
        assert_eq!(sanitize_query("*something"), "something");
        assert_eq!(sanitize_query("**something"), "something");
        assert_eq!(sanitize_query("?something"), "something");
    }

    #[test]
    fn test_preserve_trailing_wildcards() {
        assert_eq!(sanitize_query("something*"), "something*");
        assert_eq!(sanitize_query("some*thing"), "some*thing");
    }

    #[test]
    fn test_preserve_normal_queries() {
        assert_eq!(sanitize_query("UserService"), "UserService");
        assert_eq!(sanitize_query("  UserService  "), "UserService");
    }
}

// ============================================================================
// Full Preprocessing Pipeline Tests
// ============================================================================

#[cfg(test)]
mod preprocessing_pipeline {
    use super::*;

    #[test]
    fn test_preprocess_symbol_query() {
        let result = preprocess_query("UserService").unwrap();
        assert_eq!(result.query_type, QueryType::Symbol);
        assert_eq!(result.original, "UserService");
    }

    #[test]
    fn test_preprocess_pattern_query() {
        let result = preprocess_query("async fn").unwrap();
        assert_eq!(result.query_type, QueryType::Pattern);
        assert_eq!(result.original, "async fn");
    }

    #[test]
    fn test_preprocess_glob_query() {
        let result = preprocess_query("*.rs").unwrap();
        assert_eq!(result.query_type, QueryType::Glob);
        assert_eq!(result.original, "*.rs");
    }

    #[test]
    fn test_preprocess_standard_query() {
        let result = preprocess_query("error handling logic").unwrap();
        assert_eq!(result.query_type, QueryType::Standard);
        assert_eq!(result.original, "error handling logic");
    }

    #[test]
    fn test_preprocess_rejects_pure_wildcard() {
        assert!(preprocess_query("*").is_err());
        assert!(preprocess_query("**").is_err());
    }

    #[test]
    fn test_preprocess_rejects_empty_query() {
        assert!(preprocess_query("").is_err());
        assert!(preprocess_query("   ").is_err());
    }

    #[test]
    fn test_preprocess_preserves_original_query() {
        // Tantivy handles all the fancy parsing, so we just preserve the original
        let original = "class CmsService : ICmsService";
        let result = preprocess_query(original).unwrap();
        assert_eq!(result.original, original);
    }

    #[test]
    fn test_preprocess_with_special_characters() {
        // With Tantivy, we don't need to sanitize special characters
        // They're passed directly to Tantivy which handles them properly
        let query = "std::vector<T>";
        let result = preprocess_query(query).unwrap();
        assert_eq!(result.query_type, QueryType::Pattern);
        assert_eq!(result.original, query);
    }

    #[test]
    fn test_preprocess_with_regex_operators() {
        // Tantivy handles regex operators in queries
        let query = "InputFile.*spreadsheet";
        let result = preprocess_query(query).unwrap();
        assert_eq!(result.query_type, QueryType::Pattern);
        // Original is preserved - Tantivy will parse it
        assert_eq!(result.original, query);
    }

    #[test]
    fn test_preprocess_csharp_nullable() {
        // C# nullable syntax - preserved for Tantivy
        let query = "string?";
        let result = preprocess_query(query).unwrap();
        // Could be Pattern or Standard depending on detection
        assert!(result.query_type == QueryType::Pattern || result.query_type == QueryType::Standard);
        assert_eq!(result.original, query);
    }

    #[test]
    fn test_preprocess_file_glob_patterns() {
        let patterns = vec!["*.rs", "*.ts", "*.py", "**/src/**/*.java"];
        for pattern in patterns {
            let result = preprocess_query(pattern).unwrap();
            assert_eq!(result.query_type, QueryType::Glob);
            assert_eq!(result.original, pattern);
        }
    }

    #[test]
    fn test_preprocess_code_patterns() {
        let patterns = vec!["[Test]", "=>", "::", "async fn", "impl Trait"];
        for pattern in patterns {
            let result = preprocess_query(pattern).unwrap();
            assert_eq!(result.query_type, QueryType::Pattern);
            assert_eq!(result.original, pattern);
        }
    }

    #[test]
    fn test_preprocess_various_symbol_formats() {
        let symbols = vec![
            "getUserData",
            "get_user_data",
            "GetUserData",
            "MAX_BUFFER_SIZE",
            "myFunction123",
        ];
        for symbol in symbols {
            let result = preprocess_query(symbol).unwrap();
            assert_eq!(result.query_type, QueryType::Symbol);
            assert_eq!(result.original, symbol);
        }
    }
}
