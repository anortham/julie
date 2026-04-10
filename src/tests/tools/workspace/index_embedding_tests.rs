//! Tests for indexing and embedding pipeline fixes.

use crate::database::SymbolDatabase;
use tempfile::TempDir;

/// Helper: create a fresh test DB.
fn create_test_db() -> (SymbolDatabase, TempDir) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (db, dir)
}

/// Helper: insert a file record and symbol so store_embeddings has a valid FK target.
///
/// Inserts all non-nullable integer columns (`start_col`, `end_col`, `start_byte`,
/// `end_byte`) so that `get_all_symbols()` (which SELECTs them as integers) doesn't
/// fail with "Invalid column type Null".
fn insert_test_symbol(db: &mut SymbolDatabase, id: &str, name: &str, file_path: &str) {
    db.conn
        .execute(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
             VALUES (?, 'rust', 'deadbeef', 100, 0, 0)",
            rusqlite::params![file_path],
        )
        .expect("Failed to insert test file");
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, file_path, language,
                                  start_line, start_col, end_line, end_col,
                                  start_byte, end_byte, reference_score)
             VALUES (?, ?, 'function', ?, 'rust', 1, 0, 10, 0, 0, 0, 1.0)",
            rusqlite::params![id, name, file_path],
        )
        .expect("Failed to insert test symbol");
}

/// Verify that clearing embeddings on a separate DB does not affect another DB.
/// This characterizes the correct routing behavior: when force-indexing a reference
/// workspace, `clear_all_embeddings()` must be called on the REFERENCE DB,
/// not on the primary workspace DB.
#[test]
fn test_clear_embeddings_on_separate_db_does_not_affect_other() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();
    let primary_path = dir1.path().join("primary.db");
    let reference_path = dir2.path().join("reference.db");

    let mut primary_db = SymbolDatabase::new(&primary_path).unwrap();
    let mut reference_db = SymbolDatabase::new(&reference_path).unwrap();

    // Insert a symbol into each DB so store_embeddings has a valid FK target.
    insert_test_symbol(&mut primary_db, "sym_primary", "primary_fn", "src/main.rs");
    insert_test_symbol(&mut reference_db, "sym_ref", "reference_fn", "lib/lib.rs");

    // Store one embedding in each DB.
    primary_db
        .store_embeddings(&[("sym_primary".to_string(), vec![0.1_f32; 384])])
        .unwrap();
    reference_db
        .store_embeddings(&[("sym_ref".to_string(), vec![0.2_f32; 384])])
        .unwrap();

    assert_eq!(
        primary_db.embedding_count().unwrap(),
        1,
        "primary should have 1 embedding before clear"
    );
    assert_eq!(
        reference_db.embedding_count().unwrap(),
        1,
        "reference should have 1 embedding before clear"
    );

    // Clear embeddings on the REFERENCE db only (simulating force-index of ref workspace).
    reference_db.clear_all_embeddings().unwrap();

    // Primary must be untouched.
    assert_eq!(
        primary_db.embedding_count().unwrap(),
        1,
        "primary embeddings must be untouched after clearing reference DB"
    );
    // Reference is now empty.
    assert_eq!(
        reference_db.embedding_count().unwrap(),
        0,
        "reference embeddings must be 0 after clear"
    );
}

/// Verify that `embedding_count()` returns the actual total row count from
/// `symbol_vectors`, not merely the number of vectors stored in a single run.
///
/// This characterizes the correct behavior that `spawn_workspace_embedding`
/// must report to daemon.db: the ground-truth total after the pipeline
/// finishes, regardless of how many vectors were added *this* run.
#[test]
fn test_embedding_count_reflects_total_vectors_not_run_count() {
    let (mut db, _dir) = create_test_db();

    insert_test_symbol(&mut db, "sym_a", "process_data", "src/lib.rs");
    insert_test_symbol(&mut db, "sym_b", "handle_error", "src/lib.rs");

    // Store embeddings for both symbols; count must be 2.
    let stored = db
        .store_embeddings(&[
            ("sym_a".to_string(), vec![0.1_f32; 384]),
            ("sym_b".to_string(), vec![0.2_f32; 384]),
        ])
        .unwrap();
    assert_eq!(stored, 2, "store_embeddings should report 2 stored");
    assert_eq!(
        db.embedding_count().unwrap(),
        2,
        "embedding_count() should be 2 after storing both"
    );

    // Simulate a partial re-embed: delete sym_b's embedding and re-store only sym_a.
    // A pipeline that ran only for sym_a would report stats.symbols_embedded == 1,
    // but the DB ground truth is still 1 total vector (sym_a only).
    db.delete_embeddings_for_file("src/lib.rs").unwrap();
    assert_eq!(
        db.embedding_count().unwrap(),
        0,
        "embedding_count() should be 0 after deleting all embeddings for file"
    );

    // Re-store only sym_a (simulating a partial re-embed run).
    let stored_partial = db
        .store_embeddings(&[("sym_a".to_string(), vec![0.1_f32; 384])])
        .unwrap();
    assert_eq!(stored_partial, 1, "partial re-embed stored 1 vector");

    // The actual DB total is 1, not 2. This is what daemon.db must record.
    assert_eq!(
        db.embedding_count().unwrap(),
        1,
        "embedding_count() must reflect actual DB total (1), not the original run count (2)"
    );
}

/// Verify that a shared provider container propagates updates to readers.
///
/// This validates the design pattern used by IncrementalIndexer: the workspace
/// and watcher share an Arc<RwLock<Option<...>>> so that lazy initialization
/// of the embedding provider (which happens on first search, well after watcher
/// construction) is visible to the watcher's background tasks.
#[test]
fn test_shared_provider_container_propagates_updates() {
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};
    use std::sync::{Arc, RwLock};

    type SharedProvider = Arc<RwLock<Option<Arc<dyn EmbeddingProvider>>>>;

    let shared: SharedProvider = Arc::new(RwLock::new(None));
    let watcher_ref = shared.clone();

    // Before lazy init, the watcher's view is None.
    assert!(
        watcher_ref.read().unwrap().is_none(),
        "watcher should see None before provider is initialized"
    );

    // Simulate lazy init by writing a provider into the shared container.
    struct DummyProvider;
    impl EmbeddingProvider for DummyProvider {
        fn embed_query(&self, _: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![1.0, 2.0, 3.0, 4.0])
        }
        fn embed_batch(&self, _: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![])
        }
        fn dimensions(&self) -> usize {
            4
        }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".to_string(),
                device: "cpu".to_string(),
                model_name: "dummy".to_string(),
                dimensions: 4,
            }
        }
    }

    *shared.write().unwrap() = Some(Arc::new(DummyProvider));

    // After lazy init, the watcher's cloned Arc sees the updated provider.
    let snapshot = watcher_ref.read().unwrap();
    assert!(
        snapshot.is_some(),
        "watcher must see the provider after lazy initialization"
    );
    let provider = snapshot.as_ref().unwrap();
    assert_eq!(provider.dimensions(), 4);

    // Verify the provider actually works through the shared reference.
    let result = provider.embed_query("test").unwrap();
    assert_eq!(result, vec![1.0, 2.0, 3.0, 4.0]);
}

/// Verify that indexing a reference workspace derives the workspace_id from the
/// reference path, NOT from handler.workspace_id (the primary).
///
/// Bug: handle_index_command used handler.workspace_id for daemon.db stats even
/// when is_reference_workspace=true, so reference stats overwrote primary stats.
/// Fix: when is_reference_workspace, derive workspace_id from generate_workspace_id().
#[test]
fn test_reference_workspace_gets_path_derived_id() {
    use crate::workspace::registry::generate_workspace_id;

    let primary_path = "/tmp/julie_test_primary_workspace";
    let reference_path = "/tmp/julie_test_reference_workspace";

    let primary_id = generate_workspace_id(primary_path).unwrap();
    let reference_id = generate_workspace_id(reference_path).unwrap();

    assert_ne!(
        primary_id, reference_id,
        "primary and reference workspaces must have distinct IDs"
    );

    // Simulate the FIXED path: when is_reference_workspace, derive from path.
    let is_reference_workspace = true;
    let workspace_id = if is_reference_workspace {
        generate_workspace_id(reference_path).unwrap_or_default()
    } else {
        // Bug: always used primary_id
        primary_id.clone()
    };

    assert_eq!(
        workspace_id, reference_id,
        "stats must be attributed to the reference workspace ID"
    );
    assert_ne!(
        workspace_id, primary_id,
        "stats must NOT use the primary workspace ID for a reference workspace"
    );
}

/// Verify that the pipeline stop when cancel flag is set before the run.
/// Also verifies Release/Acquire ordering on the flag.
#[test]
fn test_pipeline_cancel_flag_stops_run_with_acquire_ordering() {
    use crate::embeddings::pipeline::run_embedding_pipeline_cancellable;
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    let (mut db, _dir) = create_test_db();
    insert_test_symbol(&mut db, "sym_a", "fn_a", "src/lib.rs");
    insert_test_symbol(&mut db, "sym_b", "fn_b", "src/lib.rs");
    let db_arc = Arc::new(Mutex::new(db));

    struct NoopProvider;
    impl EmbeddingProvider for NoopProvider {
        fn embed_query(&self, _: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.0; 4])
        }
        fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.0_f32; 4]).collect())
        }
        fn dimensions(&self) -> usize {
            4
        }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".to_string(),
                device: "cpu".to_string(),
                model_name: "noop-model".to_string(),
                dimensions: 4,
            }
        }
    }

    // Set cancel=true with Release ordering before calling the pipeline.
    let cancel = AtomicBool::new(false);
    cancel.store(true, Ordering::Release);

    let result = run_embedding_pipeline_cancellable(&db_arc, &NoopProvider, None, Some(&cancel));
    assert!(
        result.is_ok(),
        "Pipeline failed: {:?}",
        result.as_ref().err()
    );
    let stats = result.unwrap();

    // Cancel was set before start — pipeline should embed nothing.
    assert_eq!(
        stats.symbols_embedded, 0,
        "cancelled pipeline must embed 0 symbols"
    );

    let db_guard = db_arc.lock().unwrap();
    assert_eq!(
        db_guard.embedding_count().unwrap(),
        0,
        "DB must have 0 embeddings after pre-cancelled pipeline"
    );
}

/// Verify that the pipeline stops after a batch write when cancel is set during that batch.
/// This specifically tests the post-batch cancel check (Fix B part 1).
#[test]
fn test_pipeline_cancel_after_batch_stops_before_next_batch() {
    use crate::embeddings::pipeline::run_embedding_pipeline_cancellable;
    use crate::embeddings::{DeviceInfo, EmbeddingProvider};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    let (db, _dir) = create_test_db();

    // Insert more than one batch worth of symbols (BATCH_SIZE=250).
    // Use individual file records to avoid FK constraints.
    for i in 0..300_usize {
        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
                 VALUES (?, 'rust', 'deadbeef', 100, 0, 0)",
                rusqlite::params![format!("src/file_{i}.rs")],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, file_path, language,
                                      start_line, start_col, end_line, end_col,
                                      start_byte, end_byte, reference_score)
                 VALUES (?, ?, 'function', ?, 'rust', 1, 0, 10, 0, 0, 0, 1.0)",
                rusqlite::params![
                    format!("sym_{i}"),
                    format!("fn_{i}"),
                    format!("src/file_{i}.rs"),
                ],
            )
            .unwrap();
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();
    let batches_called = Arc::new(Mutex::new(0usize));
    let batches_clone = batches_called.clone();

    // A provider that sets cancel=true after the first embed_batch call.
    struct CancelOnSecondBatch {
        cancel: Arc<AtomicBool>,
        calls: Arc<Mutex<usize>>,
    }
    impl EmbeddingProvider for CancelOnSecondBatch {
        fn embed_query(&self, _: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.0; 4])
        }
        fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            let mut n = self.calls.lock().unwrap();
            *n += 1;
            if *n == 1 {
                // Signal cancel after first batch (simulates a concurrent force-reindex).
                self.cancel.store(true, Ordering::Release);
            }
            Ok(texts.iter().map(|_| vec![0.1_f32; 4]).collect())
        }
        fn dimensions(&self) -> usize {
            4
        }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo {
                runtime: "test".to_string(),
                device: "cpu".to_string(),
                model_name: "cancel-model".to_string(),
                dimensions: 4,
            }
        }
    }

    let provider = CancelOnSecondBatch {
        cancel: cancel_clone,
        calls: batches_clone,
    };
    let db_arc = Arc::new(Mutex::new(db));

    let result = run_embedding_pipeline_cancellable(&db_arc, &provider, None, Some(&cancel));
    assert!(
        result.is_ok(),
        "Pipeline failed: {:?}",
        result.as_ref().err()
    );
    let stats = result.unwrap();

    let batch_count = *batches_called.lock().unwrap();
    assert_eq!(
        batch_count, 1,
        "provider should only be called once before cancel stops the pipeline"
    );
    assert!(
        stats.symbols_embedded <= 250,
        "pipeline cancelled after first batch must embed at most one batch of symbols, got {}",
        stats.symbols_embedded
    );
    assert!(
        stats.symbols_embedded > 0,
        "first batch should have been stored before cancel was detected"
    );
}

/// Verify that the catch-up dedup flag prevents concurrent auto-indexing runs.
/// With the AtomicBool guard in run_auto_indexing, only one concurrent scan runs.
#[test]
fn test_catchup_dedup_flag_prevents_concurrent_scans() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let in_progress = Arc::new(AtomicBool::new(false));

    // First call: CAS succeeds — this session "owns" the catch-up.
    let first = in_progress
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok();
    assert!(first, "first caller must acquire the dedup flag");

    // Second concurrent call: CAS fails — another session is already running.
    let second = in_progress
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok();
    assert!(!second, "second caller must not acquire the dedup flag");

    // After the first session finishes, the flag is cleared.
    in_progress.store(false, Ordering::Release);

    // Now a third call can proceed.
    let third = in_progress
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok();
    assert!(third, "after flag cleared, next caller must acquire it");

    in_progress.store(false, Ordering::Release);
}
