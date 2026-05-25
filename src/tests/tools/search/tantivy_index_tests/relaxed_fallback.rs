use tempfile::TempDir;

use crate::search::index::{SearchDocument, SearchFilter, SearchIndex, SymbolSearchResults};

#[test]
fn test_or_fallback_returns_partial_matches() {
    // When searching for multiple terms where no single symbol contains ALL of them,
    // OR mode should still return symbols that match SOME terms, ranked by match count.
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // Symbol that matches "ranking" and "score" (2 of 4 terms)
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "apply_ranking_score",
            "pub fn apply_ranking_score(results: &mut Vec<SearchResult>)",
            "Apply ranking scores to search results",
            "fn apply_ranking_score(results: &mut Vec<SearchResult>) { /* impl */ }",
            "src/search/scoring.rs",
            "function",
            "rust",
            10,
        ))
        .unwrap();

    // Symbol that matches "centrality" and "boost" (2 of 4 terms)
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "2",
            "apply_centrality_boost",
            "pub fn apply_centrality_boost(results: &mut Vec<SearchResult>)",
            "Boost results by graph centrality",
            "fn apply_centrality_boost(results: &mut Vec<SearchResult>) { /* impl */ }",
            "src/search/scoring.rs",
            "function",
            "rust",
            30,
        ))
        .unwrap();

    // Symbol that matches only "score" (1 of 4 terms)
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "3",
            "calculate_score",
            "pub fn calculate_score(input: &str) -> f32",
            "Calculate a score",
            "fn calculate_score(input: &str) -> f32 { 0.0 }",
            "src/scoring.rs",
            "function",
            "rust",
            50,
        ))
        .unwrap();

    index.commit().unwrap();

    // search_symbols auto-falls-back to OR when AND returns nothing.
    // "ranking score boost centrality" — no symbol has ALL four tokens, so AND
    // fails and OR kicks in, returning partial matches.
    let auto_results = index
        .search_symbols(
            "ranking score boost centrality",
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert!(
        !auto_results.results.is_empty(),
        "search_symbols should auto-fallback to OR and return partial matches"
    );
    // Both 2-term matches should appear before the 1-term match
    assert!(
        auto_results.results.len() >= 2,
        "Should find at least the two 2-term matches, got {}",
        auto_results.results.len()
    );
    assert!(
        auto_results.relaxed,
        "relaxed should be true when OR fallback was used"
    );

    // Explicit OR mode via search_symbols_relaxed should return the same results
    let or_results = index
        .search_symbols_relaxed(
            "ranking score boost centrality",
            &SearchFilter::default(),
            10,
        )
        .unwrap();
    assert!(
        !or_results.results.is_empty(),
        "OR mode should return partial matches"
    );
    assert_eq!(
        auto_results.results.len(),
        or_results.results.len(),
        "Auto-fallback and explicit OR should return same number of results"
    );
    assert!(
        or_results.relaxed,
        "search_symbols_relaxed should always return relaxed = true"
    );
}

#[test]
fn test_search_symbols_auto_fallback_to_or() {
    // search_symbols should automatically fall back to OR when AND returns zero results
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "apply_ranking_score",
            "pub fn apply_ranking_score()",
            "Ranking scores",
            "fn apply_ranking_score() {}",
            "src/scoring.rs",
            "function",
            "rust",
            10,
        ))
        .unwrap();

    index.commit().unwrap();

    // Query with terms that partially match — AND would fail, OR should succeed
    let results = index
        .search_symbols("ranking boost centrality", &SearchFilter::default(), 10)
        .unwrap();

    assert!(
        !results.results.is_empty(),
        "search_symbols should auto-fallback to OR when AND returns nothing"
    );
    assert_eq!(results.results[0].name, "apply_ranking_score");
    assert!(
        results.relaxed,
        "relaxed should be true when OR fallback was used"
    );
}

#[test]
fn test_search_symbols_prefers_and_when_available() {
    // When AND produces results, OR fallback should NOT be used
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "UserService",
            "pub struct UserService",
            "",
            "pub struct UserService {}",
            "src/user.rs",
            "class",
            "rust",
            1,
        ))
        .unwrap();

    index.commit().unwrap();

    // Single-term query — AND works fine, no fallback needed
    let results = index
        .search_symbols("UserService", &SearchFilter::default(), 10)
        .unwrap();
    assert_eq!(results.results[0].name, "UserService");
}

#[test]
fn test_search_symbols_relaxed_flag_false_when_and_matches() {
    // When AND mode finds results, relaxed should be false
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "UserService",
            "pub struct UserService",
            "",
            "pub struct UserService {}",
            "src/user.rs",
            "class",
            "rust",
            1,
        ))
        .unwrap();
    index.commit().unwrap();

    let result: SymbolSearchResults = index
        .search_symbols("UserService", &SearchFilter::default(), 10)
        .unwrap();

    assert!(!result.results.is_empty(), "Should find UserService");
    assert!(
        !result.relaxed,
        "relaxed should be false when AND mode found results"
    );
}

#[test]
fn test_search_symbols_relaxed_flag_true_on_or_fallback() {
    // When AND mode returns nothing and OR fallback kicks in, relaxed should be true
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "apply_ranking_score",
            "pub fn apply_ranking_score()",
            "Ranking scores",
            "fn apply_ranking_score() {}",
            "src/scoring.rs",
            "function",
            "rust",
            10,
        ))
        .unwrap();
    index.commit().unwrap();

    // "ranking boost centrality" — symbol only matches "ranking", not all three terms.
    // AND fails, OR fallback kicks in → relaxed should be true
    let result: SymbolSearchResults = index
        .search_symbols("ranking boost centrality", &SearchFilter::default(), 10)
        .unwrap();

    assert!(
        !result.results.is_empty(),
        "Should find partial matches via OR fallback"
    );
    assert!(
        result.relaxed,
        "relaxed should be true when OR fallback was used"
    );
}

#[test]
fn test_search_symbols_relaxed_always_true() {
    // search_symbols_relaxed should always return relaxed = true
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "UserService",
            "pub struct UserService",
            "",
            "pub struct UserService {}",
            "src/user.rs",
            "class",
            "rust",
            1,
        ))
        .unwrap();
    index.commit().unwrap();

    let result: SymbolSearchResults = index
        .search_symbols_relaxed("UserService", &SearchFilter::default(), 10)
        .unwrap();

    assert!(!result.results.is_empty(), "Should find UserService");
    assert!(
        result.relaxed,
        "search_symbols_relaxed should always return relaxed = true"
    );
}

#[test]
fn test_content_search_or_fallback_when_and_returns_nothing() {
    // When searching content for multiple terms where no single file contains ALL of them,
    // OR fallback should kick in and return files matching SOME terms.
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // File containing "tantivy" but NOT "postgresql"
    index
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/search/engine.rs",
            "use tantivy::collector::TopDocs;\nlet searcher = index.reader();",
            "rust",
        ))
        .unwrap();

    // File containing "postgresql" but NOT "tantivy"
    index
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/database/pg.rs",
            "let conn = postgresql::connect(\"localhost\");\nlet rows = conn.query();",
            "rust",
        ))
        .unwrap();

    index.commit().unwrap();

    // "tantivy postgresql" — no file has BOTH terms, so AND returns nothing
    let result = index
        .search_content("tantivy postgresql", &SearchFilter::default(), 10)
        .unwrap();

    assert!(
        !result.results.is_empty(),
        "OR fallback should return partial matches when AND finds nothing"
    );
    assert!(
        result.relaxed,
        "relaxed should be true when OR fallback was used"
    );
    // Both files should be returned since each matches one term
    assert!(
        result.results.len() >= 2,
        "Should find both files via OR fallback, got {}",
        result.results.len()
    );
}

#[test]
fn test_content_search_relaxed_false_when_and_matches() {
    // When AND mode finds results, relaxed should be false
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // File containing BOTH "tantivy" and "search"
    index
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/search/engine.rs",
            "use tantivy::collector::TopDocs;\nfn search() { /* impl */ }",
            "rust",
        ))
        .unwrap();

    index.commit().unwrap();

    // "tantivy search" — the file has BOTH terms, so AND succeeds
    let result = index
        .search_content("tantivy search", &SearchFilter::default(), 10)
        .unwrap();

    assert!(
        !result.results.is_empty(),
        "AND mode should find the file with both terms"
    );
    assert!(
        !result.relaxed,
        "relaxed should be false when AND mode succeeded"
    );
}

#[test]
fn test_content_search_single_term_no_fallback() {
    // Single-term queries should never trigger OR fallback (no point — AND and OR are identical)
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/main.rs",
            "fn main() { println!(\"hello\"); }",
            "rust",
        ))
        .unwrap();

    index.commit().unwrap();

    // Single term "nonexistent" — no match, but should NOT trigger fallback
    let result = index
        .search_content("nonexistent", &SearchFilter::default(), 10)
        .unwrap();

    assert!(
        result.results.is_empty(),
        "Should find nothing for nonexistent term"
    );
    assert!(
        !result.relaxed,
        "relaxed should be false for single-term queries even with no results"
    );
}
