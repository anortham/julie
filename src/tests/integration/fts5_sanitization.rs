// FTS5 Query Sanitization Tests
//
// Tests for handling special FTS5 characters that cause syntax errors
//
// Bug: User queries containing special characters like #, [, ], @, etc. cause FTS5 syntax errors
// Root cause: FTS5 interprets these as operators (# for column specs, @ for auxiliary functions)
// Fix: Sanitize or quote queries containing special characters

use crate::database::SymbolDatabase;
use tempfile::TempDir;

#[test]
fn test_fts5_hash_symbol_in_query() {
    // Reproduction: Searching for "#[test]" causes "fts5: syntax error near '#'"
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).expect("Failed to create database");

    // This should NOT panic or return an error about syntax
    let result = db.find_symbols_by_pattern("#[test]", None);

    match result {
        Ok(symbols) => {
            // Empty results are OK - we just care that it doesn't crash
            assert!(
                symbols.is_empty(),
                "Expected empty results for non-existent pattern"
            );
        }
        Err(e) => {
            // Fail if we get an FTS5 syntax error
            let error_msg = e.to_string();
            assert!(
                !error_msg.contains("fts5") && !error_msg.contains("syntax error"),
                "FTS5 syntax error not properly sanitized: {}",
                error_msg
            );
        }
    }
}

#[test]
fn test_fts5_column_specifier_in_query() {
    // Reproduction: Query like "extract column" might be interpreted as "extract:column"
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).expect("Failed to create database");

    // Search for "fts5 extract" - the error was "no such column: extract"
    let result = db.find_symbols_by_pattern("fts5 extract", None);

    match result {
        Ok(symbols) => {
            assert!(
                symbols.is_empty(),
                "Expected empty results for non-existent pattern"
            );
        }
        Err(e) => {
            let error_msg = e.to_string();
            assert!(
                !error_msg.contains("no such column"),
                "Query incorrectly interpreted as column specifier: {}",
                error_msg
            );
        }
    }
}

#[test]
fn test_fts5_special_characters() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).expect("Failed to create database");

    // Test various special characters that FTS5 might misinterpret
    let special_queries = vec![
        "#[test]",      // Rust attribute
        "@Component",   // Decorator
        "[typescript]", // Brackets
        "C++",          // Plus signs
        "async/await",  // Forward slash
        "foo:bar",      // Colon (column specifier)
        "println!",     // Exclamation mark (Rust macro)
        "eprintln!",    // Exclamation mark
        "func(arg)",    // Parentheses (function call)
        "(a || b)",     // Parentheses (grouping)
        "System.Collections.Generic", // Dot (namespace/qualified name)
        "CurrentUserService.ApplicationUser", // Dot (qualified name)
    ];

    for query in special_queries {
        let result = db.find_symbols_by_pattern(query, None);

        match result {
            Ok(_) => {
                // Success - query was properly sanitized
            }
            Err(e) => {
                let error_msg = e.to_string();
                panic!(
                    "Query '{}' caused FTS5 error (should be sanitized): {}",
                    query, error_msg
                );
            }
        }
    }
}

#[test]
fn test_fts5_intentional_operators_preserved() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).expect("Failed to create database");

    // These should be passed through as-is (intentional FTS5 operators)
    let operator_queries = vec![
        "\"exact phrase\"", // Quoted phrase
        "foo*",             // Prefix wildcard
        "foo AND bar",      // Explicit AND
        "foo OR bar",       // Explicit OR
        "foo NOT bar",      // Explicit NOT
    ];

    for query in operator_queries {
        let result = db.find_symbols_by_pattern(query, None);

        // These should execute without syntax errors
        // (they might return empty results, but shouldn't crash)
        assert!(
            result.is_ok(),
            "Intentional operator query '{}' should work: {:?}",
            query,
            result
        );
    }
}
