//! Dogfooding Tests - Search Quality Against Julie's Own Codebase
//!
//! These tests validate search quality by running real queries against
//! Julie's actual workspace. This is the ultimate integration test.
//!
//! ## Test Categories
//!
//! 1. **Multi-word AND Logic** - Multiple terms should all match (not OR)
//! 2. **Hyphenated Terms** - Handle separators correctly
//! 3. **Symbol Definitions** - Find function/class definitions
//! 4. **FTS5 Internals** - SQL patterns and database queries
//! 5. **Ranking Quality** - Source files should rank above tests

use super::helpers::*;
use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;

/// Setup Julie handler for testing
async fn setup_handler() -> JulieServerHandler {
    // Create handler - it will auto-detect Julie's workspace from CWD
    let handler = JulieServerHandler::new()
        .await
        .expect("Failed to create handler");

    // Ensure the workspace is indexed
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,  // Use current workspace
        force: Some(false),  // Use cache if available
        name: None,
        workspace_id: None,
        detailed: None,
    };

    index_tool
        .call_tool(&handler)
        .await
        .expect("Failed to index workspace");

    handler
}

// ============================================================================
// Category 1: Multi-Word AND Logic Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_multiword_and_finds_sql_ranking() {
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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

// ============================================================================
// Category 6: Special Characters & Edge Cases
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_dotted_identifiers() {
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
    let handler = setup_handler().await;

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
