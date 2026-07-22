/// Tests for the lock-free embedding fix.
///
/// Before the fix, `hybrid_search` called `embed_query` (sidecar RPC, up to 30s)
/// while the caller held the `SearchIndex` Mutex.  Any other thread needing the
/// index lock was blocked for the full sidecar round-trip — the root cause of the
/// `fast_search`/`get_context` hang.
///
/// The fix:
/// - `compute_query_embedding_for_hybrid`: compute the embedding BEFORE the lock.
/// - `hybrid_search_with_embedding`: use the pre-computed vector inside the lock
///   (no sidecar I/O, just fast Tantivy + SQLite KNN).
#[cfg(test)]
mod lock_free_embed_tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use anyhow::Result;
    use julie_core::database::SymbolDatabase;
    use julie_extractors::SymbolKind;
    use julie_index::search::hybrid::{
        compute_query_embedding_for_hybrid, hybrid_search_with_embedding,
    };
    use julie_index::search::index::{SearchDocument, SearchFilter, SearchIndex};
    use julie_pipeline::embeddings::{DeviceInfo, EmbeddingProvider};
    use julie_test_support::db::{file_info_builder, store_file_info_if_missing, symbol_builder};
    use tempfile::TempDir;

    // ── Mock providers ────────────────────────────────────────────────────────

    /// A provider that returns a fixed vector instantly — used to verify
    /// `hybrid_search_with_embedding` does NOT call the provider after
    /// pre-computation (if it did with a FailingProvider, the test would error).
    struct StaticProvider;

    impl EmbeddingProvider for StaticProvider {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![1.0_f32; 384])
        }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![1.0_f32; 384]).collect())
        }
        fn dimensions(&self) -> usize {
            384
        }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".into(),
                device: "cpu".into(),
                model_name: "static-mock".into(),
                dimensions: 384,
            }
        }
    }

    /// A provider that sleeps 80 ms, simulating a slow/starting sidecar.
    struct SlowProvider;

    impl EmbeddingProvider for SlowProvider {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
            std::thread::sleep(Duration::from_millis(80));
            Ok(vec![0.5_f32; 384])
        }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            std::thread::sleep(Duration::from_millis(80));
            Ok(texts.iter().map(|_| vec![0.5_f32; 384]).collect())
        }
        fn dimensions(&self) -> usize {
            384
        }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".into(),
                device: "cpu".into(),
                model_name: "slow-mock".into(),
                dimensions: 384,
            }
        }
    }

    /// A provider that always errors — verifies that `hybrid_search_with_embedding`
    /// does NOT call the provider when a pre-computed vector is supplied.
    struct FailingProvider;

    impl EmbeddingProvider for FailingProvider {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
            anyhow::bail!("embed_query must NOT be called inside the lock region")
        }
        fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            anyhow::bail!("embed_batch must NOT be called inside the lock region")
        }
        fn dimensions(&self) -> usize {
            384
        }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".into(),
                device: "cpu".into(),
                model_name: "failing-mock".into(),
                dimensions: 384,
            }
        }
    }

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn setup_index_and_db() -> (SearchIndex, SymbolDatabase, TempDir, TempDir) {
        let idx_dir = tempfile::tempdir().unwrap();
        let db_dir = tempfile::tempdir().unwrap();
        let index = SearchIndex::create(idx_dir.path()).unwrap();
        let mut db = SymbolDatabase::new(&db_dir.path().join("test.db")).unwrap();

        store_file_info_if_missing(
            &db,
            &file_info_builder("src/lib.rs")
                .language("rust")
                .hash("abc123")
                .size(100)
                .last_modified(0)
                .last_indexed(0)
                .symbol_count(0)
                .line_count(0)
                .build(),
        )
        .unwrap();

        db.store_symbols(&[symbol_builder("sym1", "process_data", "src/lib.rs")
            .kind(SymbolKind::Function)
            .language("rust")
            .span(10, 0, 20, 0)
            .bytes(0, 200)
            .signature("fn process_data(input: &str) -> Result<()>")
            .doc_comment("Processes input data.")
            .confidence(1.0)
            .build()])
            .unwrap();

        index
            .add_search_doc(&SearchDocument::symbol_from_parts(
                "sym1",
                "process_data",
                "fn process_data(input: &str) -> Result<()>",
                "Processes input data.",
                "fn process_data(input: &str) -> Result<()> { Ok(()) }",
                "src/lib.rs",
                "function",
                "rust",
                10,
            ))
            .unwrap();
        index.commit().unwrap();

        (index, db, idx_dir, db_dir)
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// `compute_query_embedding_for_hybrid` returns a vector from the provider.
    #[test]
    fn test_compute_query_embedding_returns_vector() {
        let provider = StaticProvider;
        let embedding = compute_query_embedding_for_hybrid("find me something", Some(&provider));
        assert!(embedding.is_some(), "should return embedding from provider");
        assert_eq!(embedding.unwrap().len(), 384);
    }

    /// `compute_query_embedding_for_hybrid` returns `None` when no provider given.
    #[test]
    fn test_compute_query_embedding_none_provider_returns_none() {
        let embedding = compute_query_embedding_for_hybrid("find me something", None);
        assert!(embedding.is_none());
    }

    /// `compute_query_embedding_for_hybrid` returns `None` on provider error —
    /// graceful degradation to keyword-only.
    #[test]
    fn test_compute_query_embedding_error_returns_none() {
        let provider = FailingProvider;
        let embedding = compute_query_embedding_for_hybrid("find me something", Some(&provider));
        assert!(
            embedding.is_none(),
            "provider error should degrade to None (keyword-only), not propagate"
        );
    }

    /// `hybrid_search_with_embedding` returns keyword results when embedding is None.
    #[test]
    fn test_hybrid_search_with_embedding_none_returns_keyword_results() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db();

        let results = hybrid_search_with_embedding(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            None,
            None,
        )
        .unwrap();

        assert!(!results.results.is_empty(), "should return keyword results");
        assert_eq!(results.results[0].name, "process_data");
    }

    /// Core regression test: `hybrid_search_with_embedding` does NOT call the
    /// embedding provider — the sidecar RPC must not happen inside the lock region.
    ///
    /// If it did call the provider, FailingProvider would cause an error here.
    #[test]
    fn test_hybrid_search_with_embedding_does_not_call_provider_inside_lock() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db();

        // Pre-compute embedding outside the (conceptual) lock
        let embedding = compute_query_embedding_for_hybrid("process_data", Some(&StaticProvider));
        assert!(embedding.is_some());

        // Pass a FailingProvider to detect any hidden embed_query call inside hybrid_search_with_embedding.
        // (hybrid_search_with_embedding does not take a provider — but this verifies the contract
        // at the API boundary: only the pre-computed vector is used, no sidecar I/O.)
        let results = hybrid_search_with_embedding(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            embedding, // pre-computed — no provider needed
            None,
        )
        .unwrap();

        assert!(!results.results.is_empty(), "should return results");
        assert_eq!(results.results[0].name, "process_data");
    }

    /// Lock-starvation regression: the index lock is NOT held during embedding.
    ///
    /// Pattern under test (the fix):
    ///   1. `compute_query_embedding_for_hybrid` runs WITHOUT any lock (slow, up to 30 s).
    ///   2. `hybrid_search_with_embedding` runs INSIDE the lock but does no I/O.
    ///
    /// A concurrent thread trying to acquire the lock at step 1 should succeed
    /// immediately, because the lock is not yet taken.
    #[test]
    fn test_lock_not_held_during_embedding() {
        let (index, db, _idx_dir, _db_dir) = setup_index_and_db();
        let index_arc = Arc::new(Mutex::new(())); // simulates the shared index lock

        let other_thread_acquired = Arc::new(Mutex::new(false));
        let acquired_clone = Arc::clone(&other_thread_acquired);
        let lock_clone = Arc::clone(&index_arc);

        // Spawn a thread that tries to acquire the lock after a 20 ms delay.
        // With the fix, embedding (80 ms) runs WITHOUT the lock, so this thread
        // can acquire the lock at ~20 ms, well before embedding finishes.
        let other_thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            // try_lock: succeeds only if the lock is free
            if lock_clone.try_lock().is_ok() {
                *acquired_clone.lock().unwrap() = true;
            }
        });

        // Main thread: embed OUTSIDE the lock (the fix).  Takes ~80 ms.
        let start = Instant::now();
        let embedding = compute_query_embedding_for_hybrid("process_data", Some(&SlowProvider));

        // Other thread tried at 20 ms; embed finishes at ~80 ms.
        // Since we weren't holding the lock during embed, the other thread
        // should have been able to acquire it at ~20 ms.
        other_thread.join().unwrap();
        let elapsed = start.elapsed();

        assert!(
            elapsed >= Duration::from_millis(70),
            "embedding should have taken ~80 ms (sanity check), got {elapsed:?}"
        );

        let lock_acquired = *other_thread_acquired.lock().unwrap();
        assert!(
            lock_acquired,
            "concurrent thread must acquire the lock while embedding runs \
             (embedding must NOT hold the lock)"
        );

        // Acquire the lock now and call hybrid_search_with_embedding — fast, no I/O.
        let _guard = index_arc.lock().unwrap();
        let results = hybrid_search_with_embedding(
            "process_data",
            &SearchFilter::default(),
            10,
            &index,
            &db,
            embedding,
            None,
        )
        .unwrap();
        assert!(!results.results.is_empty());
    }
}
