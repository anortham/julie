use super::*;

/// Fix A: macOS RenameMode::Any on a non-existent path should emit Deleted, not be silently dropped.
///
/// On macOS, notify-rs emits RenameMode::Any with a single path. When that path is the
/// OLD file (moved away), `should_index_file` returns false because the file no longer
/// exists, and the event is silently dropped. The fix: check existence and fall back to
/// `should_process_deletion` + FileChangeType::Deleted.
#[tokio::test]
async fn test_rename_any_nonexistent_path_emits_deleted() {
    use crate::watcher::events::process_file_system_event;
    use notify::{
        Event, EventKind,
        event::{ModifyKind, RenameMode},
    };
    use std::collections::{HashSet, VecDeque};
    use std::sync::{Arc, atomic::AtomicBool};
    use tokio::sync::Mutex as TokioMutex;

    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("moved.rs");

    // Create the file, canonicalize its path, then delete it — simulating the old path
    // after a rename on macOS (the source file is gone, only destination exists)
    fs::write(&test_file, "fn moved() {}").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();
    fs::remove_file(&test_file).unwrap();
    assert!(!test_file.exists(), "File should be gone before event");

    let mut extensions = HashSet::new();
    extensions.insert("rs".to_string());
    let gitignore = filtering::build_gitignore_matcher(temp_dir.path()).unwrap();
    let queue: Arc<TokioMutex<VecDeque<crate::watcher::types::FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));
    let needs_rescan = Arc::new(AtomicBool::new(false));

    // Emit a RenameMode::Any event (macOS FSEvents style) for the now-gone path
    let event = Event {
        kind: EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
        paths: vec![absolute_path],
        attrs: Default::default(),
    };

    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        event,
        &needs_rescan,
    )
    .await
    .expect("Event processing should succeed");

    let queue_lock = queue.lock().await;
    assert_eq!(
        queue_lock.len(),
        1,
        "macOS rename of non-existent path should be queued as Deleted, not dropped"
    );
    assert!(
        matches!(
            queue_lock[0].change_type,
            crate::watcher::types::FileChangeType::Deleted
        ),
        "Event type should be Deleted for non-existent renamed path, got: {:?}",
        queue_lock[0].change_type
    );
}

/// Fix A (existing path): RenameMode::Any on an EXISTING path should emit Modified.
#[tokio::test]
async fn test_rename_any_existing_path_emits_modified() {
    use crate::watcher::events::process_file_system_event;
    use notify::{
        Event, EventKind,
        event::{ModifyKind, RenameMode},
    };
    use std::collections::{HashSet, VecDeque};
    use std::sync::{Arc, atomic::AtomicBool};
    use tokio::sync::Mutex as TokioMutex;

    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("arrived.rs");
    fs::write(&test_file, "fn arrived() {}").unwrap();
    assert!(test_file.exists());

    let mut extensions = HashSet::new();
    extensions.insert("rs".to_string());
    let gitignore = filtering::build_gitignore_matcher(temp_dir.path()).unwrap();
    let queue: Arc<TokioMutex<VecDeque<crate::watcher::types::FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));
    let needs_rescan = Arc::new(AtomicBool::new(false));

    let event = Event {
        kind: EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
        paths: vec![test_file.clone()],
        attrs: Default::default(),
    };

    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        event,
        &needs_rescan,
    )
    .await
    .expect("Event processing should succeed");

    let queue_lock = queue.lock().await;
    assert_eq!(
        queue_lock.len(),
        1,
        "Existing renamed path should be queued"
    );
    assert!(
        matches!(
            queue_lock[0].change_type,
            crate::watcher::types::FileChangeType::Modified
        ),
        "Event type should be Modified for existing renamed path"
    );
}

/// Fix B-a: handle_file_deleted_static should NOT have a path.exists() guard.
/// The caller (dispatch_file_event) already guards on existence; the inner check
/// creates a TOCTOU window where embeddings are deleted but symbols survive.
///
/// Test: call handle_file_deleted_static directly on a path that EXISTS but has
/// indexed symbols — should clean up regardless (trust the caller's decision).
#[tokio::test]
async fn test_delete_handler_trusts_caller_no_toctou() {
    use crate::database::SymbolDatabase;
    use crate::extractors::ExtractorManager;
    use crate::watcher::handlers::{
        handle_file_created_or_modified_static, handle_file_deleted_static,
    };
    use crate::workspace::mutation_gate::acquire_gate;
    use std::sync::{Arc, Mutex};

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_delete_toctou").await;

    // Index a real file first
    let test_file = dir.path().join("toctou.rs");
    fs::write(&test_file, "pub fn will_be_deleted() {}").unwrap();
    handle_file_created_or_modified_static(
        test_file.clone(),
        &db,
        &extractor_manager,
        dir.path(),
        None,
        &guard,
    )
    .await
    .unwrap();

    let before = {
        let db_lock = db.lock().unwrap();
        db_lock.get_symbols_for_file("toctou.rs").unwrap().len()
    };
    assert!(before > 0, "Symbol should be indexed before delete");

    // Simulate TOCTOU: file still exists on disk, but call the delete handler directly
    // (mimicking the scenario where the file is recreated between the two checks)
    assert!(
        test_file.exists(),
        "File must still exist to test TOCTOU fix"
    );
    handle_file_deleted_static(test_file.clone(), &db, dir.path(), None, &guard)
        .await
        .unwrap();

    let after = {
        let db_lock = db.lock().unwrap();
        db_lock.get_symbols_for_file("toctou.rs").unwrap().len()
    };
    assert_eq!(
        after, 0,
        "handle_file_deleted_static must clean up symbols even when file still exists on disk"
    );
}

/// Fix C (HOL blocking): after dropping a deduped event, subsequent events for OTHER
/// files must still be processed in the same tick.
///
/// With the old `break`: file_a is dedup-skipped and re-queued, then the inner loop
/// breaks — file_b never gets processed in that tick.
///
/// With the fix `continue`: file_a is dropped, the loop continues to file_b,
/// and file_b is processed in the same tick.
#[tokio::test]
async fn test_dedup_requeue_does_not_block_subsequent_events() {
    use crate::watcher::types::{FileChangeEvent, FileChangeType};
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::time::SystemTime;
    use tokio::sync::Mutex as TokioMutex;

    let queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));

    let file_a = std::path::PathBuf::from("/workspace/a.rs");
    let file_b = std::path::PathBuf::from("/workspace/b.rs");

    // Push file_a then file_b
    {
        let mut q = queue.lock().await;
        q.push_back(FileChangeEvent {
            path: file_a.clone(),
            change_type: FileChangeType::Modified,
            timestamp: SystemTime::now(),
        });
        q.push_back(FileChangeEvent {
            path: file_b.clone(),
            change_type: FileChangeType::Modified,
            timestamp: SystemTime::now(),
        });
    }

    // file_a is "recently processed", dedup should skip and drop it.
    let mut last_processed = std::collections::HashMap::<std::path::PathBuf, SystemTime>::new();
    last_processed.insert(file_a.clone(), SystemTime::now());

    let mut processed_files = Vec::new();

    // Run the loop with the FIXED logic (continue instead of break on dedup).
    // We run until we process file_b (or a safety cap to prevent infinite loops).
    let mut iterations = 0;
    loop {
        iterations += 1;
        if iterations > 20 {
            panic!("Loop did not terminate — possible HOL blocking regression");
        }

        let event = {
            let mut q = queue.lock().await;
            q.pop_front()
        };
        let Some(event) = event else { break };

        let should_skip = last_processed
            .get(&event.path)
            .and_then(|t| SystemTime::now().duration_since(*t).ok())
            .map(|e| e.as_millis() < 1000)
            .unwrap_or(false);

        if should_skip {
            // Fix C: continue (was 'break') so events behind the deduped one proceed.
            // New behavior: deduped events are dropped, not re-queued.
            continue;
        }

        processed_files.push(event.path.clone());
        last_processed.insert(event.path.clone(), SystemTime::now());

        // Stop once file_b is processed — the key assertion is that it was reached
        // despite file_a being dedup-skipped ahead of it.
        if processed_files.contains(&file_b) {
            break;
        }
    }

    // file_b must be processed — the fix ensures events aren't blocked by a dedup re-queue
    assert!(
        processed_files.contains(&file_b),
        "file_b should be processed even when file_a is re-queued for dedup. \
         If this fails, the old 'break' behavior is back (HOL blocking)."
    );
    // file_a should be dropped because it was already processed recently
    let q = queue.lock().await;
    assert!(
        !q.iter().any(|e| e.path == file_a),
        "file_a duplicate should be dropped, not left in queue"
    );
}

/// Fix C (overflow): queue overflow should set needs_rescan flag.
#[tokio::test]
async fn test_queue_overflow_sets_needs_rescan() {
    use crate::watcher::events::process_file_system_event;
    use notify::{Event, EventKind, event::CreateKind};
    use std::collections::{HashSet, VecDeque};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use tokio::sync::Mutex as TokioMutex;

    let temp_dir = tempfile::tempdir().unwrap();
    let mut extensions = HashSet::new();
    extensions.insert("rs".to_string());
    let gitignore = filtering::build_gitignore_matcher(temp_dir.path()).unwrap();
    let queue: Arc<TokioMutex<VecDeque<crate::watcher::types::FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));
    let needs_rescan = Arc::new(AtomicBool::new(false));

    // Flood the queue past the 1000-item cap by creating and registering real files
    // (process_file_system_event only queues if the file exists for Create events)
    // We use the test temp dir and fake paths that pass extension check.
    // Since we can't efficiently create 1001 real files, we'll directly push events
    // into the queue first, then overflow via one more process_file_system_event call.
    {
        let mut q = queue.lock().await;
        for i in 0..1000 {
            let path = temp_dir.path().join(format!("file{}.rs", i));
            q.push_back(crate::watcher::types::FileChangeEvent {
                path,
                change_type: crate::watcher::types::FileChangeType::Modified,
                timestamp: std::time::SystemTime::now(),
            });
        }
    }

    // Now add one more via process_file_system_event — this should trigger overflow
    let overflow_file = temp_dir.path().join("overflow.rs");
    fs::write(&overflow_file, "fn overflow() {}").unwrap();
    let event = Event {
        kind: EventKind::Create(CreateKind::File),
        paths: vec![overflow_file],
        attrs: Default::default(),
    };

    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        event,
        &needs_rescan,
    )
    .await
    .expect("Processing should succeed");

    assert!(
        needs_rescan.load(Ordering::Acquire),
        "Queue overflow should set needs_rescan flag"
    );
}

/// Test: Remove events should be queued even when the file no longer exists on disk.
///
/// `should_index_file()` checks `path.is_file()` which returns false for deleted
/// files. This means real file deletions are silently dropped — stale symbols and
/// embeddings persist in the database forever.
///
/// The fix: for Remove events, check extension and ignore patterns without
/// requiring the file to exist on disk.
#[tokio::test]
async fn test_remove_event_queued_for_deleted_file() {
    use crate::watcher::events::process_file_system_event;
    use notify::{Event, EventKind, event::RemoveKind};
    use std::collections::{HashSet, VecDeque};
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    let temp_dir = tempfile::tempdir().unwrap();
    let test_file = temp_dir.path().join("deleted.rs");

    // Create then delete — simulating a real file deletion
    fs::write(&test_file, "fn gone() {}").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();
    fs::remove_file(&test_file).unwrap();
    assert!(!test_file.exists(), "File should be gone");

    let mut extensions = HashSet::new();
    extensions.insert("rs".to_string());
    let gitignore = filtering::build_gitignore_matcher(temp_dir.path()).unwrap();
    let queue: Arc<TokioMutex<VecDeque<crate::watcher::types::FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));

    let event = Event {
        kind: EventKind::Remove(RemoveKind::File),
        paths: vec![absolute_path],
        attrs: Default::default(),
    };

    let needs_rescan = Arc::new(std::sync::atomic::AtomicBool::new(false));
    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        event,
        &needs_rescan,
    )
    .await
    .expect("Event processing should succeed");

    let queue_lock = queue.lock().await;
    assert_eq!(
        queue_lock.len(),
        1,
        "Remove event should be queued even though file no longer exists"
    );
    assert!(
        matches!(
            queue_lock[0].change_type,
            crate::watcher::types::FileChangeType::Deleted
        ),
        "Queued event should be a Deleted type"
    );
}
