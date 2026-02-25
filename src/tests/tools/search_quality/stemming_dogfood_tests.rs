//! Stemming & Namespace Dogfood Tests
//!
//! These tests validate that stemming improves search recall and that
//! namespace/module symbols don't dominate search results.
//!
//! **ALL TESTS ARE `#[ignore]`** because the fixture database must be rebuilt
//! with stemmed Tantivy indexes before they can pass. Once the fixture is
//! rebuilt, remove `#[ignore]` and they become regression guards.
//!
//! Run them with:
//!
//! ```bash
//! cargo test --lib stemming_dogfood -- --ignored   # Run ignored tests
//! cargo test --lib stemming_dogfood --no-run       # Just compile-check
//! ```

use super::helpers::*;

// ============================================================================
// Stemming Quality Tests
// ============================================================================

/// Search "token estimation" should find TokenEstimator / estimate symbols
/// thanks to stemming: "estimation" -> "estim", "estimator" -> "estim".
///
/// Without stemming, "estimation" won't match "estimator" because they share
/// no exact token. Stemming collapses both to the same root.
#[ignore] // Requires fixture rebuild with stemmed index
#[tokio::test(flavor = "multi_thread")]
async fn test_stemming_estimation_finds_estimator() {
    let handler = setup_handler_with_fixture().await;

    let results = search_content(&handler, "token estimation", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);

    // Should find TokenEstimator thanks to estimation->estim matching estimator->estim
    let has_estimator = results.iter().any(|r| {
        r.file_path.contains("token_estimation") || r.name.to_lowercase().contains("estimat")
    });
    assert!(
        has_estimator,
        "Should find estimator-related results for 'estimation' query. \
         Stemming should map estimation->estim and estimator->estim:\n{}",
        format_result_list(&results)
    );
}

/// Search "formatting output" should find format/formatter symbols
/// thanks to stemming: "formatting" -> "format", "formatter" -> "format".
///
/// This tests that verb forms and agent nouns converge to the same stem.
#[ignore] // Requires fixture rebuild with stemmed index
#[tokio::test(flavor = "multi_thread")]
async fn test_stemming_formatting_finds_formatter() {
    let handler = setup_handler_with_fixture().await;

    let results = search_content(&handler, "formatting output", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);

    let has_format = results
        .iter()
        .any(|r| r.name.to_lowercase().contains("format") || r.file_path.contains("format"));
    assert!(
        has_format,
        "Should find formatter-related results via stemming. \
         'formatting' and 'formatter' should both stem to 'format':\n{}",
        format_result_list(&results)
    );
}

// ============================================================================
// Namespace De-boost Tests
// ============================================================================

/// Searching for a module name should return actual code symbols,
/// not just the namespace/module declaration dominating the results.
///
/// For example, searching "token_estimation" should find the structs,
/// methods, and functions inside that module -- not just the `mod token_estimation`
/// declaration as the only/top result.
#[ignore] // Requires fixture rebuild with stemmed index
#[tokio::test(flavor = "multi_thread")]
async fn test_namespace_not_dominant_in_results() {
    let handler = setup_handler_with_fixture().await;

    let results = search_definitions(&handler, "token_estimation", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);

    // The results should include actual code symbols, not just the namespace declaration
    let has_non_namespace = results.iter().any(|r| {
        let kind = r.kind.to_string();
        kind != "namespace" && kind != "module"
    });
    assert!(
        has_non_namespace,
        "Should find actual code symbols (structs, functions, methods), \
         not just namespace/module declarations:\n{}",
        format_result_list(&results)
    );
}

// ============================================================================
// Helper
// ============================================================================

/// Format results for readable assertion failure messages.
fn format_result_list(results: &[crate::extractors::Symbol]) -> String {
    if results.is_empty() {
        return "  (no results)".to_string();
    }

    results
        .iter()
        .enumerate()
        .map(|(i, r)| format!("  [{}] {} ({}) in {}", i + 1, r.name, r.kind, r.file_path))
        .collect::<Vec<_>>()
        .join("\n")
}
