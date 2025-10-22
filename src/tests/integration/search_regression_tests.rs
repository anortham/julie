//! Search Regression Tests
//!
//! Tests for recurring search issues that keep biting us in production.
//! Each test represents a real bug found in the CoA Intranet project.
//!
//! Reference: TODO.md - Investigation Results section

use crate::database::SymbolDatabase;
use crate::extractors::{Symbol, SymbolKind};
use crate::tools::search::matches_glob_pattern;
use tempfile::TempDir;

// ============================================================================
// ISSUE 1: Glob Pattern Matching Failures
// ============================================================================
//
// Root Cause: Specific filenames with ** don't work, even though extension
// patterns do. Hypothesis: UNC paths (\\?\C:\...) aren't normalized before
// glob matching.
//
// Working patterns: *.cs, **/*.cs, *RfaFormPageV2.razor
// Broken patterns: **/Program.cs, Program.cs, *Program.cs, **/path/file.ext

#[cfg(test)]
mod glob_pattern_regression {
    use super::*;

    /// Test that specific filename with ** prefix works
    /// Bug: **/Program.cs returns no results even when Program.cs exists
    #[test]
    fn test_glob_pattern_specific_filename_with_doublestar() {
        let test_cases = vec![
            (
                "\\\\?\\C:\\source\\CoA Intranet\\CoA.Intranet.Client\\Program.cs",
                "**/Program.cs",
                true, // SHOULD match
            ),
            (
                "\\\\?\\C:\\source\\julie\\src\\main.rs",
                "**/main.rs",
                true, // SHOULD match
            ),
            (
                "\\\\?\\C:\\source\\project\\deeply\\nested\\path\\config.json",
                "**/config.json",
                true, // SHOULD match
            ),
        ];

        for (path, pattern, expected) in test_cases {
            let result = matches_glob_pattern(path, pattern);
            assert_eq!(
                result, expected,
                "Pattern '{}' should {} path '{}' but got {}",
                pattern,
                if expected { "match" } else { "NOT match" },
                path,
                result
            );
        }
    }

    /// Test that specific filename alone (no wildcards) works
    /// Bug: Program.cs returns no results even when Program.cs exists
    #[test]
    fn test_glob_pattern_specific_filename_alone() {
        let test_cases = vec![
            (
                "\\\\?\\C:\\source\\CoA Intranet\\CoA.Intranet.Client\\Program.cs",
                "Program.cs",
                true, // SHOULD match
            ),
            (
                "\\\\?\\C:\\source\\julie\\src\\main.rs",
                "main.rs",
                true, // SHOULD match
            ),
        ];

        for (path, pattern, expected) in test_cases {
            let result = matches_glob_pattern(path, pattern);
            assert_eq!(
                result, expected,
                "Pattern '{}' should {} path '{}' but got {}",
                pattern,
                if expected { "match" } else { "NOT match" },
                path,
                result
            );
        }
    }

    /// Test that wildcard prefix + specific filename works
    /// Bug: *Program.cs returns no results
    /// Note: *RfaFormPageV2.razor DOES work according to TODO.md
    #[test]
    fn test_glob_pattern_wildcard_prefix_specific_filename() {
        let test_cases = vec![
            (
                "\\\\?\\C:\\source\\CoA Intranet\\CoA.Intranet.Client\\Program.cs",
                "*Program.cs",
                true, // SHOULD match (but currently broken per TODO.md)
            ),
            (
                "\\\\?\\C:\\source\\CoA Intranet\\Pages\\RfaFormPageV2.razor",
                "*RfaFormPageV2.razor",
                true, // DOES work per TODO.md
            ),
            (
                "\\\\?\\C:\\source\\julie\\src\\main.rs",
                "*main.rs",
                true, // SHOULD match
            ),
        ];

        for (path, pattern, expected) in test_cases {
            let result = matches_glob_pattern(path, pattern);
            assert_eq!(
                result, expected,
                "Pattern '{}' should {} path '{}' but got {}",
                pattern,
                if expected { "match" } else { "NOT match" },
                path,
                result
            );
        }
    }

    /// Test that full path patterns work
    /// Bug: **/CoA.Intranet.Client/Program.cs returns no results
    #[test]
    fn test_glob_pattern_full_path_with_doublestar() {
        let test_cases = vec![
            (
                "\\\\?\\C:\\source\\CoA Intranet\\CoA.Intranet.Client\\Program.cs",
                "**/CoA.Intranet.Client/Program.cs",
                true, // SHOULD match
            ),
            (
                "\\\\?\\C:\\source\\julie\\src\\tools\\search\\mod.rs",
                "**/tools/search/mod.rs",
                true, // SHOULD match
            ),
        ];

        for (path, pattern, expected) in test_cases {
            let result = matches_glob_pattern(path, pattern);
            assert_eq!(
                result, expected,
                "Pattern '{}' should {} path '{}' but got {}",
                pattern,
                if expected { "match" } else { "NOT match" },
                path,
                result
            );
        }
    }

    /// Test that extension patterns work (these are confirmed working)
    /// Baseline: These SHOULD pass and confirm glob matching isn't completely broken
    #[test]
    fn test_glob_pattern_extension_patterns_work() {
        let test_cases = vec![
            (
                "\\\\?\\C:\\source\\CoA Intranet\\CoA.Intranet.Client\\Program.cs",
                "*.cs",
                true, // Confirmed working
            ),
            (
                "\\\\?\\C:\\source\\CoA Intranet\\CoA.Intranet.Client\\Program.cs",
                "**/*.cs",
                true, // Confirmed working
            ),
            (
                "\\\\?\\C:\\source\\julie\\src\\main.rs",
                "*.rs",
                true, // Confirmed working
            ),
            (
                "\\\\?\\C:\\source\\julie\\src\\main.rs",
                "**/*.rs",
                true, // Confirmed working
            ),
        ];

        for (path, pattern, expected) in test_cases {
            let result = matches_glob_pattern(path, pattern);
            assert_eq!(
                result, expected,
                "Pattern '{}' should {} path '{}' but got {}",
                pattern,
                if expected { "match" } else { "NOT match" },
                path,
                result
            );
        }
    }

    /// Test path normalization with different separators
    /// Hypothesis: Glob library might not handle Windows UNC paths or backslash/forward slash mixing
    #[test]
    fn test_glob_pattern_path_separator_handling() {
        let test_cases = vec![
            // Windows UNC path with backslashes
            (
                "\\\\?\\C:\\source\\project\\src\\main.rs",
                "**/*.rs",
                true,
            ),
            // Same path with forward slashes
            ("//C:/source/project/src/main.rs", "**/*.rs", true),
            // Pattern with forward slash, path with backslash
            (
                "\\\\?\\C:\\source\\project\\src\\main.rs",
                "**/src/main.rs",
                true,
            ),
            // Pattern with backslash, path with forward slash
            (
                "C:/source/project/src/main.rs",
                "**/src\\main.rs",
                true, // Should normalize and match
            ),
        ];

        for (path, pattern, expected) in test_cases {
            let result = matches_glob_pattern(path, pattern);
            assert_eq!(
                result, expected,
                "Pattern '{}' should {} path '{}' but got {}",
                pattern,
                if expected { "match" } else { "NOT match" },
                path,
                result
            );
        }
    }

    /// Test paths with spaces (common in Windows paths like "CoA Intranet")
    #[test]
    fn test_glob_pattern_paths_with_spaces() {
        let test_cases = vec![
            (
                "\\\\?\\C:\\source\\CoA Intranet\\CoA.Intranet.Client\\Program.cs",
                "**/*.cs",
                true,
            ),
            (
                "\\\\?\\C:\\source\\CoA Intranet\\CoA.Intranet.Client\\Program.cs",
                "**/CoA.Intranet.Client/*.cs",
                true,
            ),
            (
                "\\\\?\\C:\\source\\My Project\\src\\file name.rs",
                "**/file name.rs",
                true,
            ),
        ];

        for (path, pattern, expected) in test_cases {
            let result = matches_glob_pattern(path, pattern);
            assert_eq!(
                result, expected,
                "Pattern '{}' should {} path '{}' but got {}",
                pattern,
                if expected { "match" } else { "NOT match" },
                path,
                result
            );
        }
    }
}

// ============================================================================
// ISSUE 2: FTS5 Syntax Errors - Users Expect Regex, Get FTS5 Operators
// ============================================================================
//
// Root Cause: Query string passed directly to SQLite FTS5 full-text search.
// FTS5 has special syntax where `.` is a query operator.
// User expected regex pattern matching, but julie uses FTS5 text search (not regex).
//
// Example: "InputFile.*spreadsheet" → `fts5: syntax error near "."`
//
// Recommendation: Document that queries are FTS5 text search, NOT regex.
// Consider escaping special characters or adding a regex mode.

#[cfg(test)]
mod fts5_syntax_regression {
    use super::*;

    /// Test that dot character in queries doesn't cause FTS5 syntax errors
    /// Bug: "InputFile.*spreadsheet" causes "fts5: syntax error near '.'"
    #[test]
    fn test_fts5_dot_character_in_query() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("Failed to create database");

        // These queries contain `.` which FTS5 interprets as an operator
        let queries_with_dots = vec![
            "InputFile.*spreadsheet",    // Regex-like pattern (user expectation)
            "System.Collections.Generic", // .NET namespace
            "std::vector",                // C++ namespace (though :: not .)
            "React.Component",            // JavaScript qualified name
            "file.txt",                   // Filename with extension
        ];

        for query in queries_with_dots {
            let result = db.find_symbols_by_pattern(query);

            match result {
                Ok(_) => {
                    // Success - query was properly sanitized or handled
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    assert!(
                        !error_msg.contains("fts5") && !error_msg.contains("syntax error"),
                        "Query '{}' caused FTS5 syntax error (should be sanitized): {}",
                        query,
                        error_msg
                    );
                }
            }
        }
    }

    /// Test that asterisk character in queries is handled correctly
    /// FTS5 uses * for prefix matching, so this should work, but regex .* might confuse users
    #[test]
    fn test_fts5_asterisk_character_in_query() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("Failed to create database");

        let queries_with_asterisk = vec![
            "get*",          // FTS5 prefix wildcard (should work)
            "InputFile.*",   // Regex-like (user might expect regex)
            ".*",            // Regex "any character, any number of times"
        ];

        for query in queries_with_asterisk {
            let result = db.find_symbols_by_pattern(query);

            // All should execute without syntax errors
            assert!(
                result.is_ok(),
                "Query '{}' should not cause syntax error: {:?}",
                query,
                result
            );
        }
    }

    /// Test that common regex metacharacters are handled gracefully
    /// Users coming from grep/ripgrep might expect regex support
    #[test]
    fn test_fts5_regex_metacharacters() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).expect("Failed to create database");

        let regex_patterns = vec![
            r"test\d+",     // \d for digits
            r"user[0-9]+",  // Character class
            r"^start",      // Start anchor
            r"end$",        // End anchor
            r"foo|bar",     // Alternation
            r"test(ing)?",  // Optional group
            r"file\.txt",   // Escaped dot
            r"path/to/.*",  // Wildcard
        ];

        for query in regex_patterns {
            let result = db.find_symbols_by_pattern(query);

            match result {
                Ok(_) => {
                    // Query executed (might return empty results, but no crash)
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    // Should not get FTS5 syntax errors
                    assert!(
                        !error_msg.contains("fts5") && !error_msg.contains("syntax error"),
                        "Regex pattern '{}' caused FTS5 error: {}",
                        query,
                        error_msg
                    );
                }
            }
        }
    }
}

// ============================================================================
// ISSUE 3: Limit/Ranking Interaction - Relevant Results Hidden by Test Files
// ============================================================================
//
// Root Cause: Low limit (5) + test files have more matches = test results dominate,
// hiding actual implementation files.
//
// Example: Searching for "DirectusCmsService" with limit=5 returns only test files,
// hiding the actual Program.cs:63 where it's used.
//
// Solution: Either increase default limit, improve ranking to prioritize non-test files,
// or document that low limits may hide relevant results.
//
// TODO: These tests require complex database setup with bulk inserts.
// Implement after fixing glob pattern and FTS5 syntax issues.
//
// #[cfg(test)]
// mod limit_ranking_regression {
//     // Tests commented out - require database bulk insert API
// }
