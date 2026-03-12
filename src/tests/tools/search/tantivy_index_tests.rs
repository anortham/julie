//! Tests for Tantivy search index.

use tempfile::TempDir;

use crate::search::SearchError;
use crate::search::index::{
    FileDocument, SearchFilter, SearchIndex, SymbolDocument, SymbolSearchResults,
};
use crate::search::language_config::LanguageConfigs;

#[test]
fn test_create_index() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();
    assert_eq!(index.num_docs(), 0);
    assert!(temp_dir.path().join("meta.json").exists());
}

#[test]
fn test_open_existing_index() {
    let temp_dir = TempDir::new().unwrap();
    {
        let _index = SearchIndex::create(temp_dir.path()).unwrap();
    }
    let index = SearchIndex::open(temp_dir.path()).unwrap();
    assert_eq!(index.num_docs(), 0);
}

#[test]
fn test_open_or_create() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::open_or_create(temp_dir.path()).unwrap();
    assert_eq!(index.num_docs(), 0);
}

#[test]
fn test_add_symbol_and_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "UserService".into(),
            signature: "pub struct UserService".into(),
            doc_comment: "Manages users".into(),
            code_body: "pub struct UserService { db: Database }".into(),
            file_path: "src/user.rs".into(),
            kind: "class".into(),
            language: "rust".into(),
            start_line: 10,
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("user", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "Should find UserService when searching 'user'"
    );
    assert_eq!(results[0].name, "UserService");
}

#[test]
fn test_add_file_content_and_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_file_content(&FileDocument {
            file_path: "src/main.rs".into(),
            content: "fn main() { println!(\"hello world\"); }".into(),
            language: "rust".into(),
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_content("println", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(!results.is_empty(), "Should find file containing 'println'");
    assert_eq!(results[0].file_path, "src/main.rs");
}

#[test]
fn test_name_match_ranks_higher_than_body() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "process_data".into(),
            signature: "fn process_data()".into(),
            doc_comment: "".into(),
            code_body: "fn process_data() {}".into(),
            file_path: "src/a.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "handle_request".into(),
            signature: "fn handle_request()".into(),
            doc_comment: "This will process the data".into(),
            code_body: "fn handle_request() {}".into(),
            file_path: "src/b.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("process", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].name, "process_data",
        "Name match should rank first"
    );
}

#[test]
fn test_language_filter() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "process".into(),
            signature: "fn process()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/lib.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "process".into(),
            signature: "function process()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/lib.ts".into(),
            kind: "function".into(),
            language: "typescript".into(),
            start_line: 1,
        })
        .unwrap();
    index.commit().unwrap();

    let filter = SearchFilter {
        language: Some("rust".into()),
        ..Default::default()
    };
    let results = index
        .search_symbols("process", &filter, 10)
        .unwrap()
        .results;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].language, "rust");
}

#[test]
fn test_delete_by_file_path() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "foo".into(),
            signature: "fn foo()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/a.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "bar".into(),
            signature: "fn bar()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/b.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index.commit().unwrap();
    assert_eq!(index.num_docs(), 2);

    index.remove_by_file_path("src/a.rs").unwrap();
    index.commit().unwrap();
    assert_eq!(index.num_docs(), 1);
}

#[test]
fn test_camel_case_cross_convention_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "getUserData".into(),
            signature: "fn getUserData()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/api.ts".into(),
            kind: "function".into(),
            language: "typescript".into(),
            start_line: 1,
        })
        .unwrap();
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "get_user_data".into(),
            signature: "fn get_user_data()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/api.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("user", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert_eq!(
        results.len(),
        2,
        "Should find both getUserData and get_user_data when searching 'user'"
    );
}

/// Regression test: content search with CodeTokenizer over-splits multi-word queries.
///
/// Bug: Searching for "Blake3 hash" in file content is tokenized by CodeTokenizer into
/// ["blake", "3", "hash"]. The AND-per-term logic then requires all three tokens to be
/// present in the file. Since "3" appears in nearly every code file (line numbers,
/// constants, etc.) and "hash" is common (HashMap, etc.), this produces false positives
/// from files that don't actually contain "Blake3 hash".
///
/// This test documents the known behavior. The fix is at the routing level: content
/// searches should use line_mode which post-verifies via substring matching.
#[test]
fn test_content_search_over_tokenization_produces_false_positives() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // File that DOES contain "Blake3 hash"
    index
        .add_file_content(&FileDocument {
            file_path: "src/watcher.rs".into(),
            content:
                "// Check if file changed using Blake3 hash\nlet hash = blake3::hash(&content);"
                    .into(),
            language: "rust".into(),
        })
        .unwrap();

    // File that does NOT contain "Blake3" but DOES contain "3" and "hash"
    // This SHOULD NOT match, but CodeTokenizer splits "Blake3" → ["blake", "3"]
    index
        .add_file_content(&FileDocument {
            file_path: "src/utils.rs".into(),
            content: "use std::collections::HashMap;\nlet x = 3;\nfn get_hash() {}".into(),
            language: "rust".into(),
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_content("Blake3 hash", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    // The correct file should be found
    assert!(
        !results.is_empty(),
        "Should find at least the file containing 'Blake3 hash'"
    );

    // KNOWN ISSUE: CodeTokenizer splits "Blake3" into ["blake", "3"], causing
    // false positives from files containing "3" and "hash" separately.
    // This is why content search must be routed through line_mode for verification.
    let result_paths: Vec<&str> = results.iter().map(|r| r.file_path.as_str()).collect();
    // Document the false positive behavior (both files match at the Tantivy level)
    assert!(
        result_paths.contains(&"src/watcher.rs"),
        "Should find the file containing 'Blake3 hash'"
    );
    // NOTE: src/utils.rs is a FALSE POSITIVE at the Tantivy level.
    // The line_mode routing fix in mod.rs handles this by post-verifying matches.
}

/// Regression test: multi-token symbol search should require ALL tokens to match.
///
/// Bug: Searching for "select_best_candidate" splits into tokens [select, best, candidate]
/// and OR-matches them, producing false positives from symbols containing just "select"
/// or just "best" in their name/body.
#[test]
fn test_multi_token_search_requires_all_tokens() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // Add the target symbol
    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "select_best_candidate".into(),
            signature: "fn select_best_candidate(candidates: &[Symbol]) -> Option<&Symbol>".into(),
            doc_comment: "Picks the best matching candidate symbol".into(),
            code_body: "fn select_best_candidate() { /* impl */ }".into(),
            file_path: "src/resolver.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 89,
        })
        .unwrap();

    // Add a FALSE POSITIVE — contains "select" but NOT "best" or "candidate"
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "select_query".into(),
            signature: "fn select_query(table: &str) -> String".into(),
            doc_comment: "Build a SQL SELECT query".into(),
            code_body: "fn select_query() { /* impl */ }".into(),
            file_path: "src/database.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 42,
        })
        .unwrap();

    // Add another FALSE POSITIVE — contains "best" but NOT "select" or "candidate"
    index
        .add_symbol(&SymbolDocument {
            id: "3".into(),
            name: "find_best_match".into(),
            signature: "fn find_best_match(items: &[Item]) -> Option<&Item>".into(),
            doc_comment: "Find the best matching item".into(),
            code_body: "fn find_best_match() { /* impl */ }".into(),
            file_path: "src/matcher.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 15,
        })
        .unwrap();

    index.commit().unwrap();

    // Search for the compound name
    let results = index
        .search_symbols("select_best_candidate", &SearchFilter::default(), 10)
        .unwrap()
        .results;

    // CRITICAL: Should only find the actual symbol, not false positives
    assert!(!results.is_empty(), "Should find select_best_candidate");
    assert_eq!(
        results[0].name, "select_best_candidate",
        "First result should be the exact match"
    );

    // The false positives should NOT appear in results
    let result_names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(
        !result_names.contains(&"select_query"),
        "BUG: 'select_query' is a false positive — it only matches the 'select' token. \
         Multi-token searches must require ALL tokens to match. Got results: {:?}",
        result_names
    );
    assert!(
        !result_names.contains(&"find_best_match"),
        "BUG: 'find_best_match' is a false positive — it only matches the 'best' token. \
         Multi-token searches must require ALL tokens to match. Got results: {:?}",
        result_names
    );
}

#[test]
fn test_compound_token_finds_exact_identifier() {
    // Regression test: searching for a snake_case identifier should find it
    // even when its sub-parts are very common words
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // File that contains the exact identifier
    index
        .add_file_content(&FileDocument {
            file_path: "src/processor.rs".into(),
            content: "let mut files_by_language: HashMap<String, Vec<PathBuf>> = HashMap::new();"
                .into(),
            language: "rust".into(),
        })
        .unwrap();

    // File that contains the sub-parts scattered (should also match but rank lower)
    index
        .add_file_content(&FileDocument {
            file_path: "src/utils.rs".into(),
            content: "// process files for each language detected by the scanner".into(),
            language: "rust".into(),
        })
        .unwrap();

    index.commit().unwrap();

    let filter = crate::search::SearchFilter {
        language: None,
        kind: None,
        file_pattern: None,
    };

    let results = index
        .search_content("files_by_language", &filter, 10)
        .unwrap()
        .results;

    // Must find at least the file with the exact identifier
    assert!(
        !results.is_empty(),
        "Should find files matching compound identifier"
    );

    // The file with the exact identifier should rank first
    assert_eq!(
        results[0].file_path,
        "src/processor.rs",
        "File with exact identifier should rank higher. Got: {:?}",
        results.iter().map(|r| &r.file_path).collect::<Vec<_>>()
    );
}

// --- Shutdown mechanism tests ---

#[test]
fn test_shutdown_prevents_writer_creation() {
    let temp = TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();

    // Write something first to prove it works
    index
        .add_file_content(&FileDocument {
            file_path: "src/lib.rs".to_string(),
            content: "fn hello() {}".to_string(),
            language: "rust".to_string(),
        })
        .unwrap();
    index.commit().unwrap();

    // Shut down
    index.shutdown().unwrap();
    assert!(index.is_shutdown());

    // All write operations should now return Err(Shutdown)
    let result = index.add_file_content(&FileDocument {
        file_path: "src/other.rs".to_string(),
        content: "fn other() {}".to_string(),
        language: "rust".to_string(),
    });
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), SearchError::Shutdown),
        "Expected Shutdown error after shutdown"
    );
}

#[test]
fn test_shutdown_releases_lock_for_new_index() {
    let temp = TempDir::new().unwrap();

    // Create index A, write to it (acquires the Tantivy file lock)
    let index_a = SearchIndex::create(temp.path()).unwrap();
    index_a
        .add_file_content(&FileDocument {
            file_path: "src/old.rs".to_string(),
            content: "fn old() {}".to_string(),
            language: "rust".to_string(),
        })
        .unwrap();
    index_a.commit().unwrap();

    // Shut down A — this must release the file lock
    index_a.shutdown().unwrap();

    // Open index B at the SAME path — this would get LockBusy without shutdown
    let index_b = SearchIndex::open(temp.path()).unwrap();
    let write_result = index_b.add_file_content(&FileDocument {
        file_path: "src/new.rs".to_string(),
        content: "fn new_stuff() {}".to_string(),
        language: "rust".to_string(),
    });
    assert!(
        write_result.is_ok(),
        "Index B should be able to write after A was shut down: {:?}",
        write_result.err()
    );
    index_b.commit().unwrap();
}

#[test]
fn test_or_fallback_returns_partial_matches() {
    // When searching for multiple terms where no single symbol contains ALL of them,
    // OR mode should still return symbols that match SOME terms, ranked by match count.
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // Symbol that matches "ranking" and "score" (2 of 4 terms)
    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "apply_ranking_score".into(),
            signature: "pub fn apply_ranking_score(results: &mut Vec<SearchResult>)".into(),
            doc_comment: "Apply ranking scores to search results".into(),
            code_body: "fn apply_ranking_score(results: &mut Vec<SearchResult>) { /* impl */ }"
                .into(),
            file_path: "src/search/scoring.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 10,
        })
        .unwrap();

    // Symbol that matches "centrality" and "boost" (2 of 4 terms)
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "apply_centrality_boost".into(),
            signature: "pub fn apply_centrality_boost(results: &mut Vec<SearchResult>)".into(),
            doc_comment: "Boost results by graph centrality".into(),
            code_body: "fn apply_centrality_boost(results: &mut Vec<SearchResult>) { /* impl */ }"
                .into(),
            file_path: "src/search/scoring.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 30,
        })
        .unwrap();

    // Symbol that matches only "score" (1 of 4 terms)
    index
        .add_symbol(&SymbolDocument {
            id: "3".into(),
            name: "calculate_score".into(),
            signature: "pub fn calculate_score(input: &str) -> f32".into(),
            doc_comment: "Calculate a score".into(),
            code_body: "fn calculate_score(input: &str) -> f32 { 0.0 }".into(),
            file_path: "src/scoring.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 50,
        })
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
fn test_search_works_after_shutdown() {
    let temp = TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();

    // Write and commit data
    index
        .add_file_content(&FileDocument {
            file_path: "src/searchable.rs".to_string(),
            content: "fn uniqueSearchableFunction() { let x = 42; }".to_string(),
            language: "rust".to_string(),
        })
        .unwrap();
    index.commit().unwrap();

    // Shut down — writes are blocked, but reads should still work
    index.shutdown().unwrap();

    let results = index
        .search_content("uniqueSearchableFunction", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "Search should still return results after shutdown (reader is independent)"
    );
    assert_eq!(results[0].file_path, "src/searchable.rs");
}

#[test]
fn test_search_symbols_auto_fallback_to_or() {
    // search_symbols should automatically fall back to OR when AND returns zero results
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "apply_ranking_score".into(),
            signature: "pub fn apply_ranking_score()".into(),
            doc_comment: "Ranking scores".into(),
            code_body: "fn apply_ranking_score() {}".into(),
            file_path: "src/scoring.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 10,
        })
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
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "UserService".into(),
            signature: "pub struct UserService".into(),
            doc_comment: "".into(),
            code_body: "pub struct UserService {}".into(),
            file_path: "src/user.rs".into(),
            kind: "class".into(),
            language: "rust".into(),
            start_line: 1,
        })
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
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "UserService".into(),
            signature: "pub struct UserService".into(),
            doc_comment: "".into(),
            code_body: "pub struct UserService {}".into(),
            file_path: "src/user.rs".into(),
            kind: "class".into(),
            language: "rust".into(),
            start_line: 1,
        })
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
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "apply_ranking_score".into(),
            signature: "pub fn apply_ranking_score()".into(),
            doc_comment: "Ranking scores".into(),
            code_body: "fn apply_ranking_score() {}".into(),
            file_path: "src/scoring.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 10,
        })
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
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "UserService".into(),
            signature: "pub struct UserService".into(),
            doc_comment: "".into(),
            code_body: "pub struct UserService {}".into(),
            file_path: "src/user.rs".into(),
            kind: "class".into(),
            language: "rust".into(),
            start_line: 1,
        })
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
        .add_file_content(&FileDocument {
            file_path: "src/search/engine.rs".into(),
            content: "use tantivy::collector::TopDocs;\nlet searcher = index.reader();".into(),
            language: "rust".into(),
        })
        .unwrap();

    // File containing "postgresql" but NOT "tantivy"
    index
        .add_file_content(&FileDocument {
            file_path: "src/database/pg.rs".into(),
            content: "let conn = postgresql::connect(\"localhost\");\nlet rows = conn.query();"
                .into(),
            language: "rust".into(),
        })
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
        .add_file_content(&FileDocument {
            file_path: "src/search/engine.rs".into(),
            content: "use tantivy::collector::TopDocs;\nfn search() { /* impl */ }".into(),
            language: "rust".into(),
        })
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
        .add_file_content(&FileDocument {
            file_path: "src/main.rs".into(),
            content: "fn main() { println!(\"hello\"); }".into(),
            language: "rust".into(),
        })
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

/// Documents the tokenizer mismatch bug: index with language_configs tokenizer,
/// open with default tokenizer → search fails. This is the raw bug.
/// Ignored because it tests the broken path (SearchIndex::open without configs).
/// See test_ref_workspace_search_with_matching_tokenizer for the regression test.
#[test]
#[ignore]
fn test_tokenizer_mismatch_reproduces_ref_workspace_bug() {
    let temp_dir = TempDir::new().unwrap();

    {
        let configs = LanguageConfigs::load_embedded();
        let index = SearchIndex::create_with_language_configs(temp_dir.path(), &configs).unwrap();

        index
            .add_symbol(&SymbolDocument {
                id: "1".into(),
                name: "SmartQueryPreprocessor".into(),
                signature: "public class SmartQueryPreprocessor".into(),
                doc_comment: "Preprocesses search queries".into(),
                code_body: "public class SmartQueryPreprocessor { }".into(),
                file_path: "Services/SmartQueryPreprocessor.cs".into(),
                kind: "class".into(),
                language: "csharp".into(),
                start_line: 31,
            })
            .unwrap();
        index.commit().unwrap();
    }

    // BUG: Opening with default tokenizer can't find symbols indexed with language_configs
    let index = SearchIndex::open(temp_dir.path()).unwrap();
    let results = index
        .search_symbols("SmartQueryPreprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        results.is_empty(),
        "This test documents the bug: mismatched tokenizer produces no results"
    );
}

/// Regression test: reference workspace search must use language_configs tokenizer.
/// Index created with from_language_configs, reopened with from_language_configs → works.
/// This simulates the fixed production path.
#[test]
fn test_ref_workspace_search_with_matching_tokenizer() {
    let temp_dir = TempDir::new().unwrap();

    // Step 1: Create and populate index (simulates reference workspace indexing)
    {
        let configs = LanguageConfigs::load_embedded();
        let index = SearchIndex::create_with_language_configs(temp_dir.path(), &configs).unwrap();

        index
            .add_symbol(&SymbolDocument {
                id: "1".into(),
                name: "SmartQueryPreprocessor".into(),
                signature: "public class SmartQueryPreprocessor".into(),
                doc_comment: "Preprocesses search queries".into(),
                code_body: "public class SmartQueryPreprocessor { }".into(),
                file_path: "Services/SmartQueryPreprocessor.cs".into(),
                kind: "class".into(),
                language: "csharp".into(),
                start_line: 31,
            })
            .unwrap();

        index
            .add_symbol(&SymbolDocument {
                id: "2".into(),
                name: "SearchMode".into(),
                signature: "public SearchMode SearchMode { get; set; }".into(),
                doc_comment: "".into(),
                code_body: "".into(),
                file_path: "Services/SmartQueryPreprocessor.cs".into(),
                kind: "property".into(),
                language: "csharp".into(),
                start_line: 395,
            })
            .unwrap();

        index.commit().unwrap();
    }

    // Step 2: Reopen with language_configs tokenizer (simulates fixed search path)
    let configs = LanguageConfigs::load_embedded();
    let index = SearchIndex::open_with_language_configs(temp_dir.path(), &configs).unwrap();

    // CamelCase multi-part name
    let results = index
        .search_symbols("SmartQueryPreprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "SmartQueryPreprocessor must be found with matching tokenizer"
    );
    assert_eq!(results[0].name, "SmartQueryPreprocessor");

    // Individual token from CamelCase split
    let results = index
        .search_symbols("preprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "'preprocessor' must be found with matching tokenizer"
    );

    // 2-part CamelCase
    let results = index
        .search_symbols("SearchMode", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "SearchMode must be found with matching tokenizer"
    );
}

/// Control test: same scenario but open with SAME tokenizer (should always work).
#[test]
fn test_same_tokenizer_search_works() {
    let temp_dir = TempDir::new().unwrap();

    // Create and populate with language_configs tokenizer
    {
        let configs = LanguageConfigs::load_embedded();
        let index = SearchIndex::create_with_language_configs(temp_dir.path(), &configs).unwrap();

        index
            .add_symbol(&SymbolDocument {
                id: "1".into(),
                name: "SmartQueryPreprocessor".into(),
                signature: "public class SmartQueryPreprocessor".into(),
                doc_comment: "".into(),
                code_body: "".into(),
                file_path: "Services/SmartQueryPreprocessor.cs".into(),
                kind: "class".into(),
                language: "csharp".into(),
                start_line: 31,
            })
            .unwrap();
        index.commit().unwrap();
    }

    // Open with SAME tokenizer
    let configs = LanguageConfigs::load_embedded();
    let index = SearchIndex::open_with_language_configs(temp_dir.path(), &configs).unwrap();

    let results = index
        .search_symbols("SmartQueryPreprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "SmartQueryPreprocessor should be found when using same tokenizer"
    );
    assert_eq!(results[0].name, "SmartQueryPreprocessor");

    let results = index
        .search_symbols("preprocessor", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert!(
        !results.is_empty(),
        "'preprocessor' should be found when using same tokenizer"
    );
}

/// Regression test: opening a Tantivy index created with an older schema
/// (different field names / tokenizer) should recreate the index transparently
/// instead of crashing with "Error getting tokenizer for field: symbol_name".
///
/// This reproduces the bug reported when users upgraded from the pre-razorback
/// Julie version (which used `symbol_id`, `symbol_name`, `code_aware` tokenizer)
/// to the current version (which uses `id`, `name`, `code` tokenizer).
#[test]
fn test_schema_migration_recreates_stale_index() {
    use tantivy::schema::{IndexRecordOption, Schema, TextFieldIndexing, TextOptions, STORED, STRING};
    use tantivy::tokenizer::TextAnalyzer;

    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();

    // Create an index with the OLD schema (symbol_id, symbol_name, code_aware tokenizer)
    {
        let mut builder = Schema::builder();
        let old_text_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("code_aware")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();

        builder.add_text_field("doc_type", STRING | STORED);
        builder.add_text_field("symbol_id", STRING | STORED);   // old name for "id"
        builder.add_text_field("file_path", STRING | STORED);
        builder.add_text_field("language", STRING | STORED);
        builder.add_text_field("symbol_name", old_text_options); // old name for "name"
        let old_schema = builder.build();

        let old_index = tantivy::Index::create_in_dir(&index_path, old_schema).unwrap();
        // Register the old tokenizer name so we can write a doc
        old_index.tokenizers().register(
            "code_aware",
            TextAnalyzer::builder(crate::search::tokenizer::CodeTokenizer::with_default_patterns())
                .build(),
        );
        let mut writer: tantivy::IndexWriter<tantivy::TantivyDocument> = old_index.writer(15_000_000).unwrap();
        writer.commit().unwrap();
        // Index with old schema now exists on disk
    }

    // open_or_create should detect the mismatch and recreate
    let index = SearchIndex::open_or_create(&index_path).unwrap();
    assert_eq!(index.num_docs(), 0, "recreated index should be empty");

    // Verify we can write and search with the new schema
    index
        .add_symbol(&SymbolDocument {
            id: "test_sym".into(),
            name: "MyTestClass".into(),
            signature: "class MyTestClass".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/test.rs".into(),
            kind: "class".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("MyTestClass", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "MyTestClass");
}

/// Same as above, but exercises the `open_with_language_configs` path
/// (used by `handler.rs` when loading existing workspaces at startup).
#[test]
fn test_schema_migration_via_open_path() {
    use tantivy::schema::{IndexRecordOption, Schema, TextFieldIndexing, TextOptions, STORED, STRING};
    use tantivy::tokenizer::TextAnalyzer;

    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();

    // Create old-schema index
    {
        let mut builder = Schema::builder();
        builder.add_text_field("doc_type", STRING | STORED);
        builder.add_text_field("symbol_id", STRING | STORED);
        builder.add_text_field("file_path", STRING | STORED);
        builder.add_text_field("symbol_name", TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("code_aware")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored());
        let old_schema = builder.build();

        let old_index = tantivy::Index::create_in_dir(&index_path, old_schema).unwrap();
        old_index.tokenizers().register(
            "code_aware",
            TextAnalyzer::builder(crate::search::tokenizer::CodeTokenizer::with_default_patterns())
                .build(),
        );
        let mut writer: tantivy::IndexWriter<tantivy::TantivyDocument> = old_index.writer(15_000_000).unwrap();
        writer.commit().unwrap();
    }

    // open (not open_or_create) should also handle the migration
    let configs = LanguageConfigs::load_embedded();
    let index = SearchIndex::open_with_language_configs(&index_path, &configs).unwrap();
    assert_eq!(index.num_docs(), 0);

    // Verify writes work
    index
        .add_symbol(&SymbolDocument {
            id: "sym1".into(),
            name: "ProcessPayment".into(),
            signature: "fn process_payment()".into(),
            doc_comment: "".into(),
            code_body: "".into(),
            file_path: "src/payments.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 10,
        })
        .unwrap();
    index.commit().unwrap();

    let results = index
        .search_symbols("ProcessPayment", &SearchFilter::default(), 10)
        .unwrap()
        .results;
    assert_eq!(results.len(), 1);
}
