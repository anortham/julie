//! Concurrent-reader contract for `SearchIndexHandle` (`Arc<SearchIndex>`).
//!
//! `SearchIndex` is already `&self`-safe (interior writer mutex + Tantivy
//! `IndexReader`). An outer `Mutex` around the index would serialize these
//! barrier tasks (only one thread could hold the lock at the barrier) and is
//! the concurrency bug this suite guards against.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use tempfile::TempDir;

use crate::search::index::{SearchDocument, SearchFilter, SearchIndex, SearchIndexHandle};

#[test]
fn search_index_handle_supports_concurrent_readers_across_barrier() {
    let temp_dir = TempDir::new().unwrap();
    let index: SearchIndexHandle = Arc::new(SearchIndex::create(temp_dir.path()).unwrap());
    index
        .add_search_doc(&SearchDocument::symbol_from_parts(
            "1",
            "ConcurrentTarget",
            "fn concurrent_target()",
            "",
            "fn concurrent_target() {}",
            "src/lib.rs",
            "function",
            "rust",
            1,
        ))
        .unwrap();
    index.commit().unwrap();

    const N: usize = 8;
    let barrier = Arc::new(Barrier::new(N));
    let hits = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::with_capacity(N);

    for _ in 0..N {
        let index = Arc::clone(&index);
        let barrier = Arc::clone(&barrier);
        let hits = Arc::clone(&hits);
        handles.push(thread::spawn(move || {
            let filter = SearchFilter::default();
            let before = index
                .search_unified("ConcurrentTarget", &filter, 10)
                .expect("pre-barrier search");
            assert!(
                !before.is_empty(),
                "expected ConcurrentTarget hit before barrier"
            );
            // If SearchIndex were behind an outer Mutex and each task held
            // that lock across this wait, N-1 tasks could never arrive → hang.
            barrier.wait();
            let after = index
                .search_unified("ConcurrentTarget", &filter, 10)
                .expect("post-barrier search");
            assert!(!after.is_empty());
            hits.fetch_add(after.len(), Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle
            .join()
            .expect("reader thread panicked or deadlocked on outer mutex");
    }
    assert!(hits.load(Ordering::SeqCst) >= N);
}

#[test]
fn search_index_is_sync_and_send() {
    fn assert_sync<T: Sync>() {}
    fn assert_send<T: Send>() {}
    assert_sync::<SearchIndex>();
    assert_send::<SearchIndex>();
    assert_sync::<SearchIndexHandle>();
    assert_send::<SearchIndexHandle>();
}
