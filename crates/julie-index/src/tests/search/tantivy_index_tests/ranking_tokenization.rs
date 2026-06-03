use tempfile::TempDir;

use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};

#[test]
fn test_name_match_ranks_higher_than_body() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "process_data",
            "fn process_data()",
            "",
            "fn process_data() {}",
            "src/a.rs",
            "function",
            "rust",
            1,
        ))
        .unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "2",
            "handle_request",
            "fn handle_request()",
            "This will process the data",
            "fn handle_request() {}",
            "src/b.rs",
            "function",
            "rust",
            1,
        ))
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
fn test_camel_case_cross_convention_search() {
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "getUserData",
            "fn getUserData()",
            "",
            "",
            "src/api.ts",
            "function",
            "typescript",
            1,
        ))
        .unwrap();
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "2",
            "get_user_data",
            "fn get_user_data()",
            "",
            "",
            "src/api.rs",
            "function",
            "rust",
            1,
        ))
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
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/watcher.rs",
            "// Check if file changed using Blake3 hash\nlet hash = blake3::hash(&content);",
            "rust",
        ))
        .unwrap();

    // File that does NOT contain "Blake3" but DOES contain "3" and "hash"
    // This SHOULD NOT match, but CodeTokenizer splits "Blake3" → ["blake", "3"]
    index
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/utils.rs",
            "use std::collections::HashMap;\nlet x = 3;\nfn get_hash() {}",
            "rust",
        ))
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
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "select_best_candidate",
            "fn select_best_candidate(candidates: &[Symbol]) -> Option<&Symbol>",
            "Picks the best matching candidate symbol",
            "fn select_best_candidate() { /* impl */ }",
            "src/resolver.rs",
            "function",
            "rust",
            89,
        ))
        .unwrap();

    // Add a FALSE POSITIVE — contains "select" but NOT "best" or "candidate"
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "2",
            "select_query",
            "fn select_query(table: &str) -> String",
            "Build a SQL SELECT query",
            "fn select_query() { /* impl */ }",
            "src/database.rs",
            "function",
            "rust",
            42,
        ))
        .unwrap();

    // Add another FALSE POSITIVE — contains "best" but NOT "select" or "candidate"
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "3",
            "find_best_match",
            "fn find_best_match(items: &[Item]) -> Option<&Item>",
            "Find the best matching item",
            "fn find_best_match() { /* impl */ }",
            "src/matcher.rs",
            "function",
            "rust",
            15,
        ))
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
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/processor.rs",
            "let mut files_by_language: HashMap<String, Vec<PathBuf>> = HashMap::new();",
            "rust",
        ))
        .unwrap();

    // File that contains the sub-parts scattered (should also match but rank lower)
    index
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/utils.rs",
            "// process files for each language detected by the scanner",
            "rust",
        ))
        .unwrap();

    index.commit().unwrap();

    let filter = crate::search::SearchFilter {
        language: None,
        kind: None,
        file_pattern: None,
        exclude_tests: false,
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
