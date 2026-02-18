//! Tests for Tantivy search index.

use tempfile::TempDir;

use crate::search::index::{FileDocument, SearchFilter, SearchIndex, SymbolDocument};
use crate::search::SearchError;

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
        .unwrap();
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
        .unwrap();
    assert!(
        !results.is_empty(),
        "Should find file containing 'println'"
    );
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
        .unwrap();
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
    let results = index.search_symbols("process", &filter, 10).unwrap();
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
        .unwrap();
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
            content: "// Check if file changed using Blake3 hash\nlet hash = blake3::hash(&content);".into(),
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
        .unwrap();

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
        .unwrap();

    // CRITICAL: Should only find the actual symbol, not false positives
    assert!(
        !results.is_empty(),
        "Should find select_best_candidate"
    );
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
        .unwrap();

    // Must find at least the file with the exact identifier
    assert!(
        !results.is_empty(),
        "Should find files matching compound identifier"
    );

    // The file with the exact identifier should rank first
    assert_eq!(
        results[0].file_path, "src/processor.rs",
        "File with exact identifier should rank higher. Got: {:?}",
        results
            .iter()
            .map(|r| &r.file_path)
            .collect::<Vec<_>>()
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
        .unwrap();
    assert!(
        !results.is_empty(),
        "Search should still return results after shutdown (reader is independent)"
    );
    assert_eq!(results[0].file_path, "src/searchable.rs");
}
