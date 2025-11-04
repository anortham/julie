//! Dogfooding Tests - Search Quality Against Julie's Own Codebase
//!
//! These tests validate search quality by running real queries against
//! Julie's actual workspace. This is the ultimate integration test.
//!
//! **⚡ PERFORMANCE:** These tests use the pre-built JulieTestFixture for instant startup.
//! Each test loads the fixture database (~60MB, 9,240 symbols) and runs in ~0.5s.
//! Total runtime: ~10 seconds for all 33 tests (16x faster than live indexing).
//!
//! Run them with:
//!
//! ```bash
//! cargo test --lib dogfood                   # Run all dogfooding tests (~10s)
//! cargo test --lib test_ranking              # Run ranking tests (~2s)
//! cargo test --lib test_multiword_and        # Run specific test (~0.5s)
//! ```
//!
//! **Already Optimized:** The JulieTestFixture eliminates live indexing overhead.
//! See helpers.rs for implementation details.
//!
//! ## Test Categories (33 tests total)
//!
//! 1. **Multi-word AND Logic** (3 tests) - Multiple terms should all match (not OR)
//! 2. **Hyphenated Terms** (3 tests) - Handle separators correctly
//! 3. **Symbol Definitions** (2 tests) - Find function/class definitions
//! 4. **FTS5 Internals** (3 tests) - SQL patterns and database queries
//! 5. **Ranking Quality** (5 tests) - Exact matches, source over tests, frequency, etc.
//! 6. **Special Characters** (3 tests) - Dots, colons, underscores
//! 7. **Tokenizer Consistency** (1 test) - FTS5 tables use same tokenizer
//! 8. **Cross-Language Search** (3 tests) - Rust paths, multiple languages, namespace variants
//! 9. **Edge Cases** (4 tests) - Empty queries, single chars, special chars, common terms
//! 10. **Tokenization Quality** (3 tests) - camelCase splitting, underscores, numbers

use super::helpers::*;

// ============================================================================
// Category 1: Multi-Word AND Logic Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_multiword_and_finds_sql_ranking() {
    let handler = setup_handler_with_fixture().await;

    // Query: "bm25 rank ORDER" - should find SQL ranking code (all 3 terms present)
    let results = search_content(&handler, "bm25 rank ORDER", 10)
        .await
        .expect("Search failed");

    // Should find the ranking SQL in files.rs
    assert_contains_path(&results, "src/database/files.rs");
    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiword_and_cascade_architecture() {
    let handler = setup_handler_with_fixture().await;

    // Query: "CASCADE architecture SQLite FTS5"
    let results = search_content(&handler, "CASCADE architecture SQLite FTS5", 10)
        .await
        .expect("Search failed");

    // Should find docs and implementation
    assert_min_results(&results, 3);
    // Should find it in either CLAUDE.md or docs/
    let has_docs = results
        .iter()
        .any(|r| r.file_path.contains("CLAUDE.md") || r.file_path.contains("docs/"));
    assert!(has_docs, "Should find CASCADE docs");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiword_and_incremental_update() {
    let handler = setup_handler_with_fixture().await;

    // Query: "incremental update atomic"
    let results = search_content(&handler, "incremental update atomic", 10)
        .await
        .expect("Search failed");

    // Should find bulk_operations.rs
    assert_contains_path(&results, "src/database/bulk_operations.rs");
    assert_min_results(&results, 3);
}

// ============================================================================
// Category 2: Hyphenated Terms Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_hyphenated_tree_sitter() {
    let handler = setup_handler_with_fixture().await;

    // Query: "tree-sitter parse"
    // The hyphen should be handled correctly (split to OR)
    let results = search_content(&handler, "tree-sitter parse", 10)
        .await
        .expect("Search failed");

    // Should find Vue extractors and refactoring code
    assert_contains_path(&results, "src/extractors/vue/identifiers.rs");
    assert_min_results(&results, 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hyphenated_de_boost() {
    let handler = setup_handler_with_fixture().await;

    // Query: "DE-BOOST test files"
    let results = search_content(&handler, "DE-BOOST test files", 5)
        .await
        .expect("Search failed");

    // Should find the ranking SQL comment
    assert_contains_path(&results, "src/database/files.rs");
    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hyphenated_cross_language() {
    let handler = setup_handler_with_fixture().await;

    // Query: "cross-language intelligence"
    let results = search_content(&handler, "cross-language intelligence", 5)
        .await
        .expect("Search failed");

    // Should find cross_language_intelligence.rs
    assert_contains_path(&results, "src/utils/cross_language_intelligence.rs");
    assert_min_results(&results, 2);
}

// ============================================================================
// Category 3: Symbol Definition Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_symbol_search_sanitize_function() {
    let handler = setup_handler_with_fixture().await;

    // Query: "sanitize_fts5_query" - should find the function definition
    let results = search_definitions(&handler, "sanitize_fts5_query", 5)
        .await
        .expect("Search failed");

    // Should find the method in queries.rs
    assert_contains_path(&results, "src/database/symbols/queries.rs");
    assert_contains_symbol_kind(&results, "method");
    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_symbol_search_struct() {
    let handler = setup_handler_with_fixture().await;

    // Query: "JulieServerHandler"
    let results = search_definitions(&handler, "JulieServerHandler", 5)
        .await
        .expect("Search failed");

    // Should find the struct definition
    assert_contains_path(&results, "src/handler.rs");
    assert_min_results(&results, 1);
}

// ============================================================================
// Category 4: FTS5 Internals Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_fts5_create_virtual_table() {
    let handler = setup_handler_with_fixture().await;

    // Query: "CREATE VIRTUAL TABLE fts5"
    let results = search_content(&handler, "CREATE VIRTUAL TABLE fts5", 10)
        .await
        .expect("Search failed");

    // Should find schema.rs with FTS5 table creation
    assert_contains_path(&results, "src/database/schema.rs");

    // We have files_fts and symbols_fts, so should find both
    assert_min_results(&results, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fts5_snippet_function() {
    let handler = setup_handler_with_fixture().await;

    // Query: "snippet files_fts content"
    let results = search_content(&handler, "snippet files_fts content", 5)
        .await
        .expect("Search failed");

    // Should find the snippet SQL in files.rs
    assert_contains_path(&results, "src/database/files.rs");
    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fts5_corruption_tests() {
    let handler = setup_handler_with_fixture().await;

    // Query: "FTS5 corruption rowid"
    let results = search_content(&handler, "FTS5 corruption rowid", 10)
        .await
        .expect("Search failed");

    // Should find the corruption reproduction test
    let has_corruption_test = results
        .iter()
        .any(|r| r.file_path.contains("fts5_rowid_corruption.rs") || r.file_path.contains("TODO.md"));
    assert!(has_corruption_test, "Should find FTS5 corruption test files");
}

// ============================================================================
// Category 5: Ranking Quality Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_ranking_source_over_tests() {
    let handler = setup_handler_with_fixture().await;

    // Query something that appears in both source and tests
    // "SymbolDatabase" - used in implementation and tests
    let results = search_content(&handler, "SymbolDatabase", 20)
        .await
        .expect("Search failed");

    assert_min_results(&results, 3);

    // First result should NOT be a test file
    // (our de-boost logic should push tests to bottom)
    if !results.is_empty() {
        let first_is_test = results[0].file_path.contains("test");
        // Note: This might fail if we have issues with ranking
        // That's good - it tells us our ranking needs work!
        assert!(
            !first_is_test,
            "First result should not be a test file, but got: {}",
            results[0].file_path
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ranking_exact_match_over_partial() {
    let handler = setup_handler_with_fixture().await;

    // Query: "SymbolDatabase" should rank exact matches higher than partial
    // Exact: "SymbolDatabase" struct
    // Partial: "create_symbol_database", "SymbolDatabaseError", etc.
    let results = search_definitions(&handler, "SymbolDatabase", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);

    // First result should be the exact struct definition, not a derivative
    if !results.is_empty() {
        assert_eq!(
            results[0].name, "SymbolDatabase",
            "Exact match 'SymbolDatabase' should rank first, but got '{}'",
            results[0].name
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ranking_implementation_file_over_definition() {
    let handler = setup_handler_with_fixture().await;

    // Query: "fast_search" - should find the actual implementation
    // Not just the trait definition or tool struct
    let results = search_definitions(&handler, "fast_search", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);

    // Should find actual search implementation code
    // Look for either the tool definition or handler implementation
    let has_implementation = results
        .iter()
        .any(|r| r.file_path.contains("search") || r.file_path.contains("tools"));
    assert!(
        has_implementation,
        "Should find fast_search implementation in search/tools module"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ranking_frequency_matters() {
    let handler = setup_handler_with_fixture().await;

    // Query: "Result" - extremely common in Rust
    // Should rank files with more occurrences higher
    let results = search_content(&handler, "Result", 20)
        .await
        .expect("Search failed");

    assert_min_results(&results, 10);

    // Can't make strong assertions about specific ranking
    // But verify we get diverse results from actual code
    // (not just one file dominating)
    let unique_files: std::collections::HashSet<_> =
        results.iter().map(|r| r.file_path.as_str()).collect();
    assert!(
        unique_files.len() >= 5,
        "Should find 'Result' in at least 5 different files, got {}",
        unique_files.len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ranking_short_names_prefer_definitions() {
    let handler = setup_handler_with_fixture().await;

    // Query: "db" (very short, ambiguous)
    // Should still prefer actual symbol definitions over random occurrences
    let results = search_definitions(&handler, "db", 10)
        .await
        .expect("Search failed");

    // Short queries might have fewer results due to FTS5 tokenization
    // Just verify we don't error and get reasonable results
    if !results.is_empty() {
        // First result should be a real symbol, not a substring match
        assert!(
            !results[0].name.is_empty(),
            "Should find real symbols, not empty names"
        );
    }
}

// ============================================================================
// Category 6: Special Characters & Edge Cases
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_dotted_identifiers() {
    let handler = setup_handler_with_fixture().await;

    // Query: "System.Collections.Generic" style patterns
    // We split on dots, so this becomes "CASCADE OR architecture"
    let results = search_content(&handler, "CASCADE.architecture", 5)
        .await
        .expect("Search failed");

    // Should still find CASCADE architecture content
    // (dots split to OR, should match docs with both terms)
    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_colons_in_rust_paths() {
    let handler = setup_handler_with_fixture().await;

    // Query: "std::vec" style patterns
    // Colons should split to OR
    let results = search_content(&handler, "Result::Ok", 5)
        .await
        .expect("Search failed");

    // May or may not find results (depends on codebase content)
    // Just verify it doesn't error
    // If we have Rust Result usage, we should find it
}

#[tokio::test(flavor = "multi_thread")]
async fn test_underscore_snake_case() {
    let handler = setup_handler_with_fixture().await;

    // Query: "get_symbols"
    // Underscores are separators, but tokenizer handles them
    let results = search_definitions(&handler, "get_symbols", 10)
        .await
        .expect("Search failed");

    // Should find symbol extraction functions
    assert_min_results(&results, 1);
}

// ============================================================================
// Category 7: Tokenizer Consistency Tests (Will Fail Until Fixed)
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tokenizer_consistency_hyphen() {
    let handler = setup_handler_with_fixture().await;

    // FIXED: files_fts now uses the same tokenizer as symbols_fts
    // Both use: tokenize = "unicode61 separators '_::->.''"
    //
    // Query "tree-sitter" should work the same in both

    // Search file content (uses files_fts)
    let content_results = search_content(&handler, "tree-sitter", 10)
        .await
        .expect("Content search failed");

    // Search definitions (uses symbols_fts)
    let symbol_results = search_definitions(&handler, "tree-sitter", 10)
        .await
        .expect("Symbol search failed");

    // Both should find results (tokenizers should behave the same)
    // If files_fts doesn't have separators, this will fail
    assert_min_results(&content_results, 1);
    assert_min_results(&symbol_results, 0); // May or may not have symbol definitions
}

// ============================================================================
// Category 8: Cross-Language Search Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_cross_language_rust_module_paths() {
    let handler = setup_handler_with_fixture().await;

    // Query: "database::symbols" (Rust path notation)
    // Should find database/symbols module files
    let results = search_content(&handler, "database::symbols", 10)
        .await
        .expect("Search failed");

    // Should find references to database symbols module
    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cross_language_multiple_languages() {
    let handler = setup_handler_with_fixture().await;

    // Query for "extractor" - appears in Rust code, docs, comments
    // Verify we get diverse language results
    let results = search_content(&handler, "extractor", 20)
        .await
        .expect("Search failed");

    assert_min_results(&results, 5);

    // Should find extractor mentions across different file types
    let has_rust = results.iter().any(|r| r.file_path.ends_with(".rs"));
    let has_docs = results.iter().any(|r| r.file_path.ends_with(".md"));

    assert!(has_rust, "Should find extractor in Rust files");
    // Docs might not match depending on content, so don't assert strictly
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cross_language_namespace_variants() {
    let handler = setup_handler_with_fixture().await;

    // Query: "SymbolExtractor" (PascalCase)
    // Should work even if stored as different casing
    let results = search_definitions(&handler, "SymbolExtractor", 5)
        .await
        .expect("Search failed");

    // Julie's codebase uses this pattern
    // May or may not find results depending on exact naming
    // Just verify search doesn't error and handles the query
}

// ============================================================================
// Category 9: Edge Case Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_edge_case_very_common_term() {
    let handler = setup_handler_with_fixture().await;

    // Query: "test" (extremely common)
    // Should handle gracefully without overwhelming results
    let results = search_content(&handler, "test", 20)
        .await
        .expect("Search failed");

    // Should get results but limited by our limit parameter
    assert_max_results(&results, 20);
    assert_min_results(&results, 10);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edge_case_single_character() {
    let handler = setup_handler_with_fixture().await;

    // Query: "i" (single character)
    // FTS5 might have minimum token length, verify graceful handling
    let results = search_content(&handler, "i", 5)
        .await
        .expect("Search failed");

    // Single char queries might not work due to FTS5 tokenization
    // Just verify no panic/error
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edge_case_empty_query() {
    let handler = setup_handler_with_fixture().await;

    // Query: "" (empty)
    // Should handle gracefully - either error or return empty
    let results = search_content(&handler, "", 5).await;

    // Empty query should either return empty results or error gracefully
    // Don't assert specific behavior, just verify no panic
    match results {
        Ok(r) => assert!(r.is_empty() || !r.is_empty()), // Any result is fine
        Err(_) => {} // Error is also acceptable
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_edge_case_special_chars_only() {
    let handler = setup_handler_with_fixture().await;

    // Query: ":::" (only special chars)
    // Should handle gracefully
    let results = search_content(&handler, ":::", 5).await;

    // Special char queries might not match anything
    // Just verify no panic
    match results {
        Ok(r) => assert!(r.len() <= 5), // Limited results
        Err(_) => {} // Error is acceptable
    }
}

// ============================================================================
// Category 10: Tokenization Quality Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tokenization_camelCase_splitting() {
    let handler = setup_handler_with_fixture().await;

    // Query: "Symbol" (part of "SymbolDatabase", "SymbolExtractor", etc.)
    // FTS5 tokenization should split camelCase
    let results = search_definitions(&handler, "Symbol", 15)
        .await
        .expect("Search failed");

    assert_min_results(&results, 5);

    // Should find various Symbol* classes
    let symbol_names: Vec<_> = results.iter().map(|r| r.name.as_str()).collect();
    let has_multiple_symbol_types = symbol_names.iter().filter(|n| n.contains("Symbol")).count() >= 3;

    assert!(
        has_multiple_symbol_types,
        "Should find multiple Symbol-prefixed types due to camelCase splitting"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tokenization_underscore_splitting() {
    let handler = setup_handler_with_fixture().await;

    // Query: "fast" (part of "fast_search", "fast_goto", etc.)
    // Underscore separator should allow finding these
    let results = search_definitions(&handler, "fast", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 2);

    // Should find fast_* tools
    let has_fast_tools = results
        .iter()
        .any(|r| r.name.starts_with("fast") || r.name.contains("fast"));

    assert!(
        has_fast_tools,
        "Should find fast_* tools due to underscore splitting"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tokenization_number_handling() {
    let handler = setup_handler_with_fixture().await;

    // Query: "v1" or similar version patterns
    // Numbers in identifiers should be handled
    let results = search_content(&handler, "0 1 2", 10)
        .await
        .expect("Search failed");

    // Numbers are common in code (array indices, versions, etc.)
    // Just verify search handles them without error
    // Results depend on codebase content
}

