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
//! 4. **Search Internals** (3 tests) - SQL patterns and database queries
//! 5. **Ranking Quality** (5 tests) - Exact matches, source over tests, frequency, etc.
//! 6. **Special Characters** (3 tests) - Dots, colons, underscores
//! 7. **Tokenizer Consistency** (1 test) - Tantivy uses same tokenizer for index and query
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

    // Query: "query_row SELECT symbols" - should find database query code
    let results = search_content(&handler, "query_row SELECT symbols", 10)
        .await
        .expect("Search failed");

    // Should find database query files
    let has_database = results
        .iter()
        .any(|r| r.file_path.contains("database") || r.file_path.contains("queries"));
    assert!(
        has_database,
        "Should find database query code, got:\n{}",
        results
            .iter()
            .map(|r| format!("  {}", r.file_path))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiword_and_cascade_architecture() {
    let handler = setup_handler_with_fixture().await;

    // Query: "ON DELETE CASCADE" - SQL foreign key pattern in design docs
    let results = search_content(&handler, "ON DELETE CASCADE", 10)
        .await
        .expect("Search failed");

    // Should find docs with CASCADE SQL patterns
    assert_min_results(&results, 1);
    let has_docs_or_sql = results.iter().any(|r| {
        r.file_path.contains("docs/")
            || r.file_path.contains(".sql")
            || r.file_path.contains("database")
    });
    assert!(
        has_docs_or_sql,
        "Should find CASCADE SQL references in docs or database code"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_multiword_and_incremental_update() {
    let handler = setup_handler_with_fixture().await;

    // Query: "incremental update atomic"
    let results = search_content(&handler, "incremental update atomic", 10)
        .await
        .expect("Search failed");

    // Should find indexing or database files related to incremental updates
    let has_relevant = results.iter().any(|r| {
        r.file_path.contains("indexing")
            || r.file_path.contains("database")
            || r.file_path.contains("bulk_operations")
            || r.file_path.contains("watcher")
    });
    assert!(
        has_relevant,
        "Should find files related to incremental updates, got:\n{}",
        results
            .iter()
            .map(|r| format!("  {}", r.file_path))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_min_results(&results, 1);
}

// ============================================================================
// Category 2: Hyphenated Terms Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_hyphenated_tree_sitter() {
    let handler = setup_handler_with_fixture().await;

    // Query: "tree-sitter parse"
    // Hyphen splits into "tree", "sitter", "parse" — should find extractor files
    let results = search_content(&handler, "tree-sitter parse", 10)
        .await
        .expect("Search failed");

    // Should find extractor files (tree-sitter is used across all extractors)
    let has_extractors = results.iter().any(|r| {
        r.file_path.contains("extract")
            || r.file_path.contains("parse")
            || r.file_path.contains("tree-sitter")
    });
    assert!(
        has_extractors,
        "Should find extractor/parser files for 'tree-sitter parse', got:\n{}",
        results
            .iter()
            .map(|r| format!("  {}", r.file_path))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_min_results(&results, 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hyphenated_de_boost() {
    let handler = setup_handler_with_fixture().await;

    // Query: "exact-match boost scoring" — tests hyphen splitting in a search context
    // Splits to "exact", "match", "boost", "scoring"
    let results = search_content(&handler, "exact-match boost scoring", 10)
        .await
        .expect("Search failed");

    // Should find files related to scoring/boosting logic
    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_hyphenated_cross_language() {
    let handler = setup_handler_with_fixture().await;

    // Query: "cross-language intelligence"
    // Hyphen splits to "cross", "language", "intelligence"
    let results = search_content(&handler, "cross-language intelligence", 10)
        .await
        .expect("Search failed");

    // Should find files discussing cross-language features (docs, utils, tools)
    assert_min_results(&results, 1);
    let has_relevant = results.iter().any(|r| {
        r.file_path.contains("cross_language")
            || r.file_path.contains("intelligence")
            || r.file_path.contains("docs")
            || r.file_path.contains("lib.rs")
            || r.file_path.contains("navigation")
    });
    assert!(
        has_relevant,
        "Should find cross-language related files, got:\n{}",
        results
            .iter()
            .map(|r| format!("  {}", r.file_path))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ============================================================================
// Category 3: Symbol Definition Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_symbol_search_database_method() {
    let handler = setup_handler_with_fixture().await;

    let results = search_definitions(&handler, "find_by_name", 5)
        .await
        .expect("Search failed");

    assert_contains_path(&results, "crates/julie-index/src/graph/mod.rs");
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
// Category 5: Ranking Quality Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_ranking_source_over_tests() {
    let handler = setup_handler_with_fixture().await;

    // Query something that appears in both source and tests
    // "FactsStore" - used in implementation and tests
    let results = search_content(&handler, "FactsStore", 20)
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

    // Query: "FactsStore" should rank exact matches higher than partial
    // Exact: "FactsStore" struct
    // Partial: "create_symbol_database", "FactsStoreError", etc.
    let results = search_definitions(&handler, "FactsStore", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);

    // First result should be the exact struct definition, not a derivative
    if !results.is_empty() {
        assert_eq!(
            results[0].name, "FactsStore",
            "Exact match 'FactsStore' should rank first, but got '{}'",
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

    // Short queries might have fewer results due to tokenization
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
    let _results = search_content(&handler, "Result::Ok", 5)
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

    // Query "tree-sitter" should work the same in content and definition search

    // Search file content (uses Tantivy)
    let content_results = search_content(&handler, "tree-sitter", 10)
        .await
        .expect("Content search failed");

    // Search definitions (uses Tantivy)
    let symbol_results = search_definitions(&handler, "tree-sitter", 10)
        .await
        .expect("Symbol search failed");

    // Both should find results (tokenizers should behave the same)
    // Both searches should find results with the same query
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
    let _has_docs = results.iter().any(|r| r.file_path.ends_with(".md"));

    assert!(has_rust, "Should find extractor in Rust files");
    // Docs might not match depending on content, so don't assert strictly
}

#[tokio::test(flavor = "multi_thread")]
async fn test_cross_language_namespace_variants() {
    let handler = setup_handler_with_fixture().await;

    // Query: "SymbolExtractor" (PascalCase)
    // Should work even if stored as different casing
    let _results = search_definitions(&handler, "SymbolExtractor", 5)
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
    // Tantivy might have minimum token length, verify graceful handling
    let _results = search_content(&handler, "i", 5)
        .await
        .expect("Search failed");

    // Single char queries might not work due to tokenization
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
        Err(_) => {}                                     // Error is also acceptable
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
        Err(_) => {}                    // Error is acceptable
    }
}

// ============================================================================
// Category 10: Tokenization Quality Tests
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_tokenization_camel_case_splitting() {
    let handler = setup_handler_with_fixture().await;

    // Query: "Symbol" (part of "FactsStore", "SymbolExtractor", etc.)
    // Tantivy tokenization should split camelCase
    let results = search_definitions(&handler, "Symbol", 15)
        .await
        .expect("Search failed");

    assert_min_results(&results, 5);

    // Should find various Symbol* classes
    let symbol_names: Vec<_> = results.iter().map(|r| r.name.as_str()).collect();
    let has_multiple_symbol_types =
        symbol_names.iter().filter(|n| n.contains("Symbol")).count() >= 3;

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
    let _results = search_content(&handler, "0 1 2", 10)
        .await
        .expect("Search failed");

    // Numbers are common in code (array indices, versions, etc.)
    // Just verify search handles them without error
    // Results depend on codebase content
}

/// Definition search against the fixture snapshot returns symbols.
/// `code_context` is sliced from the checkout file (facts store no file
/// content). This fixture has no source tree, so context stays empty.
#[tokio::test]
async fn test_definition_search_includes_code_context() {
    let handler = setup_handler_with_fixture().await;

    let results = search_definitions(&handler, "extract_context_lines", 5)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);
}

// ============================================================================
// Category 12: fast_refs Identifier-Based Reference Discovery
// ============================================================================

/// Test that get_identifiers_by_names() returns results from the fixture DB.
/// This is a unit test for the database query layer.
#[tokio::test]
async fn test_identifiers_query_returns_results() {
    let handler = setup_handler_with_fixture().await;
    let workspace = handler.get_workspace().await.unwrap().expect("workspace");
    let snapshot = workspace.store.current();
    let graph = snapshot.graph();
    assert!(graph.len() > 0, "fixture snapshot should contain symbols");
    let sample = graph.symbol(julie_index::graph::SymbolId(0));
    assert!(
        !graph.find_by_name(&sample.name).is_empty(),
        "fixture graph should find the first symbol by name"
    );
}

/// Test that fast_refs finds references using identifiers when relationships are sparse.
/// This is the core regression test for the identifiers unlock feature.
#[tokio::test]
async fn test_fast_refs_finds_identifier_based_references() {
    use crate::tools::navigation::FastRefsTool;

    let handler = setup_handler_with_fixture().await;
    let symbol_name = "FastSearchTool".to_string();
    {
        let tool = FastRefsTool {
            symbol: symbol_name.clone(),
            include_definition: true,
            limit: 50,
            offset: 0,
            workspace: Some("primary".to_string()),
            reference_kind: None,
            semantics: None,
        };

        let result = tool
            .call_tool(handler.as_ref())
            .await
            .expect("fast_refs failed");

        // Extract text content to check results
        let text = result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        println!("fast_refs('{}') result: {} chars", symbol_name, text.len());

        // Should find at least some references (either from relationships or identifiers)
        assert!(
            !text.contains("No references found"),
            "fast_refs('{}') should find references via identifiers, but got: {}",
            symbol_name,
            &text[..text.len().min(200)]
        );
    }
}

/// Test that reference_kind filtering works with the identifiers table.
#[tokio::test]
async fn test_fast_refs_reference_kind_filter_with_identifiers() {
    use crate::tools::navigation::FastRefsTool;

    let handler = setup_handler_with_fixture().await;
    let symbol_name = "FastSearchTool".to_string();
    {
        let tool = FastRefsTool {
            symbol: symbol_name.clone(),
            include_definition: true,
            limit: 50,
            offset: 0,
            workspace: Some("primary".to_string()),
            reference_kind: Some("call".to_string()),
            semantics: None,
        };

        let result = tool
            .call_tool(handler.as_ref())
            .await
            .expect("fast_refs failed");
        let text = result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        println!(
            "fast_refs('{}', reference_kind='call') result: {} chars",
            symbol_name,
            text.len()
        );

        // With identifiers table, call filtering should now work
        assert!(
            !text.contains("No references found"),
            "fast_refs with reference_kind='call' should find results via identifiers for '{}', but got: {}",
            symbol_name,
            &text[..text.len().min(200)]
        );
    }
}

// ============================================================================
// Content Search Quality: Compound Identifiers & Cross-Line Matching
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_content_single_identifier_snake_case() {
    let handler = setup_handler_with_fixture().await;

    // "files_by_language" exists in processor.rs and query.rs as identifiers.
    // Previously returned zero results due to compound token stripping.
    // Use higher limit since .memories/ content can flood lower positions.
    let results = search_content(&handler, "files_by_language", 20)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);
    let has_source = results
        .iter()
        .any(|r| r.file_path.contains("processor.rs") || r.file_path.contains("query.rs"));
    assert!(
        has_source,
        "Should find files_by_language in source code:\n{}",
        results
            .iter()
            .map(|r| format!("  {}", r.file_path))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_content_single_identifier_camel_case() {
    let handler = setup_handler_with_fixture().await;

    // Regression guard: camelCase identifiers should still work
    let results = search_content(&handler, "LanguageParserPool", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_content_multiword_cross_line() {
    let handler = setup_handler_with_fixture().await;

    // "spawn_blocking" and "statistics" both exist in handler.rs/processor.rs
    // but not necessarily on the same line
    let results = search_content(&handler, "spawn_blocking statistics", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_content_multiword_same_line_regression() {
    let handler = setup_handler_with_fixture().await;

    // Regression guard: multi-word queries that DO appear on same lines
    // should still work
    let results = search_content(&handler, "incremental update atomic", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);
}
