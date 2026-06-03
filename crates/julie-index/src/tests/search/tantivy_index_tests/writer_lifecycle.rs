use tempfile::TempDir;

use crate::search::SearchError;
use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};

// --- Shutdown mechanism tests ---

#[test]
fn test_shutdown_prevents_writer_creation() {
    let temp = TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();

    // Write something first to prove it works
    index
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/lib.rs",
            "fn hello() {}",
            "rust",
        ))
        .unwrap();
    index.commit().unwrap();

    // Shut down
    index.shutdown().unwrap();
    assert!(index.is_shutdown());

    // All write operations should now return Err(Shutdown)
    let result = index.add_search_doc(&SearchDocument::file_from_parts(
        "src/other.rs",
        "fn other() {}",
        "rust",
    ));
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
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/old.rs",
            "fn old() {}",
            "rust",
        ))
        .unwrap();
    index_a.commit().unwrap();

    // Shut down A — this must release the file lock
    index_a.shutdown().unwrap();

    // Open index B at the SAME path — this would get LockBusy without shutdown
    let index_b = SearchIndex::open(temp.path()).unwrap();
    let write_result = index_b.add_search_doc(&SearchDocument::file_from_parts(
        "src/new.rs",
        "fn new_stuff() {}",
        "rust",
    ));
    assert!(
        write_result.is_ok(),
        "Index B should be able to write after A was shut down: {:?}",
        write_result.err()
    );
    index_b.commit().unwrap();
}

#[test]
fn test_release_writer_releases_lock_without_shutting_down_search() {
    let temp = TempDir::new().unwrap();

    let index_a = SearchIndex::create(temp.path()).unwrap();
    index_a
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/old.rs",
            "fn old_symbol() {}",
            "rust",
        ))
        .unwrap();
    index_a.release_writer().unwrap();
    assert!(
        !index_a.is_shutdown(),
        "release_writer must not disable future writes on the same SearchIndex"
    );

    let old_results = index_a
        .search_content("old_symbol", &Default::default(), 10)
        .unwrap();
    assert_eq!(
        old_results.results.len(),
        1,
        "release_writer should commit and keep the reader usable"
    );

    let index_b = SearchIndex::open(temp.path()).unwrap();
    index_b
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/new.rs",
            "fn new_symbol() {}",
            "rust",
        ))
        .unwrap();
    index_b.release_writer().unwrap();

    index_a
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/again.rs",
            "fn again_symbol() {}",
            "rust",
        ))
        .unwrap();
    index_a.commit().unwrap();
}

#[test]
fn test_search_works_after_shutdown() {
    let temp = TempDir::new().unwrap();
    let index = SearchIndex::create(temp.path()).unwrap();

    // Write and commit data
    index
        .add_search_doc(&SearchDocument::file_from_parts(
            "src/searchable.rs",
            "fn uniqueSearchableFunction() { let x = 42; }",
            "rust",
        ))
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
