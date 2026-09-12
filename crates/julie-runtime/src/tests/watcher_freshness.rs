use crate::watcher::{FileChangeEvent, FileChangeType, IncrementalIndexer};
use crate::workspace::mutation_gate::Registry;
use julie_core::indexing_state::{IndexingRuntimeState, SharedIndexingRuntime};
use julie_index::checkout_store::CheckoutStore;
use notify::event::CreateKind;
use notify::{Event, EventKind};
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

struct Fixture {
    root: tempfile::TempDir,
    _facts: tempfile::TempDir,
    store: Arc<CheckoutStore>,
    indexer: IncrementalIndexer,
    runtime: SharedIndexingRuntime,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let facts = tempfile::tempdir().unwrap();
    let store = Arc::new(
        CheckoutStore::open_with_ram_index(&facts.path().join("facts.sqlite"), root.path())
            .unwrap(),
    );
    let runtime = IndexingRuntimeState::shared();
    let indexer = IncrementalIndexer::new_with_mutation_gate_registry(
        root.path().to_path_buf(),
        Arc::clone(&store),
        Arc::new(RwLock::new(None)),
        Arc::clone(&runtime),
        Arc::new(Registry::new()),
    )
    .unwrap();
    Fixture {
        root,
        _facts: facts,
        store,
        indexer,
        runtime,
    }
}

fn symbol_names(store: &CheckoutStore, path: &str) -> Vec<String> {
    let snapshot = store.current();
    snapshot
        .graph()
        .symbols_in_path(path)
        .iter()
        .map(|id| snapshot.graph().symbol(*id).name.clone())
        .collect()
}

async fn enqueue(indexer: &IncrementalIndexer, path: std::path::PathBuf) {
    enqueue_change(indexer, path, FileChangeType::Modified).await;
}

async fn enqueue_change(
    indexer: &IncrementalIndexer,
    path: std::path::PathBuf,
    change_type: FileChangeType,
) {
    indexer.index_queue.lock().await.push_back(FileChangeEvent {
        path,
        change_type,
        timestamp: SystemTime::now(),
    });
}

#[tokio::test]
async fn watcher_second_save_inside_debounce_window_indexes_latest_content() {
    let fixture = fixture();
    let source = fixture.root.path().join("sample.rs");
    std::fs::write(&source, "fn first_version() {}\n").unwrap();
    enqueue(&fixture.indexer, source.clone()).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    std::fs::write(&source, "fn second_version() {}\nfn added_symbol() {}\n").unwrap();
    enqueue(&fixture.indexer, source).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    assert_eq!(
        symbol_names(&fixture.store, "sample.rs"),
        ["second_version", "added_symbol"]
    );
    assert_eq!(fixture.store.current().searcher().num_docs(), 3);
}

#[tokio::test]
async fn watcher_overflow_rescan_reconciles_dropped_path() {
    let fixture = fixture();
    let deleted = fixture.root.path().join("deleted.rs");
    std::fs::write(&deleted, "fn removed_from_disk() {}\n").unwrap();
    enqueue(&fixture.indexer, deleted.clone()).await;
    fixture.indexer.process_pending_changes().await.unwrap();
    std::fs::remove_file(&deleted).unwrap();

    let dropped = fixture.root.path().join("dropped.rs");
    std::fs::write(&dropped, "fn recovered_after_overflow() {}\n").unwrap();
    enqueue(&fixture.indexer, dropped).await;
    for ordinal in 1..crate::watcher::queue::MAX_QUEUE_SIZE {
        enqueue_change(
            &fixture.indexer,
            fixture.root.path().join(format!("missing-{ordinal}.rs")),
            FileChangeType::Deleted,
        )
        .await;
    }

    let retained = fixture.root.path().join("retained.rs");
    std::fs::write(&retained, "fn retained_event() {}\n").unwrap();
    let event = Event::new(EventKind::Create(CreateKind::File)).add_path(retained);
    crate::watcher::events::process_file_system_event(
        &crate::watcher::filtering::build_supported_extensions(),
        &crate::watcher::filtering::build_gitignore_matcher(fixture.root.path()).unwrap(),
        fixture.root.path(),
        Arc::clone(&fixture.indexer.index_queue),
        event,
        &fixture.indexer.needs_rescan,
    )
    .await
    .unwrap();

    assert!(fixture.indexer.needs_rescan.load(Ordering::Acquire));
    fixture.indexer.process_pending_changes().await.unwrap();

    assert_eq!(
        symbol_names(&fixture.store, "dropped.rs"),
        ["recovered_after_overflow"]
    );
    assert!(symbol_names(&fixture.store, "deleted.rs").is_empty());
    assert_eq!(
        symbol_names(&fixture.store, "retained.rs"),
        ["retained_event"]
    );
    assert!(!fixture.indexer.needs_rescan.load(Ordering::Acquire));
    assert!(
        !fixture
            .runtime
            .read()
            .unwrap()
            .snapshot()
            .watcher_rescan_pending
    );
}

#[tokio::test]
async fn watcher_failed_apply_keeps_rescan_pending_until_success() {
    let fixture = fixture();
    let source = fixture.root.path().join("retry.rs");
    std::fs::write(&source, "fn before_failure() {}\n").unwrap();
    enqueue(&fixture.indexer, source.clone()).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    std::fs::write(&source, "fn after_retry() {}\n").unwrap();
    enqueue(&fixture.indexer, source).await;
    fixture
        .indexer
        .process_pending_changes_with_commit_failure_for_test()
        .await
        .unwrap();

    assert_eq!(symbol_names(&fixture.store, "retry.rs"), ["before_failure"]);
    assert!(fixture.indexer.needs_rescan.load(Ordering::Acquire));
    assert!(
        fixture
            .runtime
            .read()
            .unwrap()
            .snapshot()
            .watcher_rescan_pending
    );

    fixture.indexer.process_pending_changes().await.unwrap();

    assert_eq!(symbol_names(&fixture.store, "retry.rs"), ["after_retry"]);
    assert!(!fixture.indexer.needs_rescan.load(Ordering::Acquire));
    assert!(
        !fixture
            .runtime
            .read()
            .unwrap()
            .snapshot()
            .watcher_rescan_pending
    );
}

#[tokio::test]
async fn watcher_rescan_preserves_unreadable_discovered_path() {
    let fixture = fixture();
    let source = fixture.root.path().join("unreadable.rs");
    std::fs::write(&source, "fn before_read_failure() {}\n").unwrap();
    enqueue(&fixture.indexer, source.clone()).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    std::fs::write(&source, "fn after_read_retry() {}\n").unwrap();
    fixture
        .indexer
        .fail_next_file_read_for_test("unreadable.rs");
    fixture.indexer.needs_rescan.store(true, Ordering::Release);
    fixture.indexer.process_pending_changes().await.unwrap();

    assert_eq!(
        symbol_names(&fixture.store, "unreadable.rs"),
        ["before_read_failure"]
    );
    assert!(fixture.indexer.needs_rescan.load(Ordering::Acquire));
    assert!(
        fixture
            .runtime
            .read()
            .unwrap()
            .snapshot()
            .watcher_rescan_pending
    );

    fixture.indexer.process_pending_changes().await.unwrap();

    assert_eq!(
        symbol_names(&fixture.store, "unreadable.rs"),
        ["after_read_retry"]
    );
    assert!(!fixture.indexer.needs_rescan.load(Ordering::Acquire));
}

#[tokio::test]
async fn watcher_queued_read_failure_schedules_reconciliation() {
    let fixture = fixture();
    let source = fixture.root.path().join("queued-read.rs");
    std::fs::write(&source, "fn before_queue_read_failure() {}\n").unwrap();
    enqueue(&fixture.indexer, source.clone()).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    std::fs::write(&source, "fn recovered_queue_read() {}\n").unwrap();
    fixture
        .indexer
        .fail_next_file_read_for_test("queued-read.rs");
    enqueue(&fixture.indexer, source).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    assert_eq!(
        symbol_names(&fixture.store, "queued-read.rs"),
        ["before_queue_read_failure"]
    );
    assert!(fixture.indexer.needs_rescan.load(Ordering::Acquire));

    fixture.indexer.process_pending_changes().await.unwrap();

    assert_eq!(
        symbol_names(&fixture.store, "queued-read.rs"),
        ["recovered_queue_read"]
    );
    assert!(!fixture.indexer.needs_rescan.load(Ordering::Acquire));
}

#[tokio::test]
async fn watcher_failed_rescan_waits_for_retry_age_while_queue_continues() {
    let fixture = fixture();
    let repair = fixture.root.path().join("repair.rs");
    std::fs::write(&repair, "fn before_delayed_retry() {}\n").unwrap();
    enqueue(&fixture.indexer, repair.clone()).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    std::fs::write(&repair, "fn after_delayed_retry() {}\n").unwrap();
    fixture.indexer.fail_next_file_read_for_test("repair.rs");
    fixture.indexer.needs_rescan.store(true, Ordering::Release);
    fixture.indexer.process_pending_changes().await.unwrap();

    let regular = fixture.root.path().join("regular.rs");
    std::fs::write(&regular, "fn regular_queue_work() {}\n").unwrap();
    enqueue(&fixture.indexer, regular).await;
    fixture
        .indexer
        .process_pending_changes_with_retry_age_for_test(std::time::Duration::from_secs(30))
        .await;

    assert_eq!(
        symbol_names(&fixture.store, "repair.rs"),
        ["before_delayed_retry"]
    );
    assert_eq!(
        symbol_names(&fixture.store, "regular.rs"),
        ["regular_queue_work"]
    );
    assert!(fixture.indexer.needs_rescan.load(Ordering::Acquire));

    fixture.indexer.process_pending_changes().await.unwrap();
    assert_eq!(
        symbol_names(&fixture.store, "repair.rs"),
        ["after_delayed_retry"]
    );
}

#[tokio::test]
async fn watcher_rescan_repairs_facts_ahead_of_graph_and_tantivy() {
    let fixture = fixture();
    let target = fixture.root.path().join("target.rs");
    let deleted = fixture.root.path().join("deleted-projection.rs");
    let staging = fixture.root.path().join("staging.rs");
    std::fs::write(&target, "fn old_target() {}\n").unwrap();
    std::fs::write(&deleted, "fn stale_deleted() {}\n").unwrap();
    std::fs::write(&staging, "fn new_target() {}\n").unwrap();
    enqueue(&fixture.indexer, target.clone()).await;
    enqueue(&fixture.indexer, deleted.clone()).await;
    enqueue(&fixture.indexer, staging.clone()).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    std::fs::write(&target, "fn new_target() {}\n").unwrap();
    std::fs::remove_file(&deleted).unwrap();
    std::fs::remove_file(&staging).unwrap();
    let connection =
        rusqlite::Connection::open(fixture._facts.path().join("facts.sqlite")).unwrap();
    let staged_hash: String = connection
        .query_row(
            "SELECT blob_hash FROM paths WHERE path = 'staging.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    connection
        .execute(
            "UPDATE paths SET blob_hash = ?1 WHERE path = 'target.rs'",
            [&staged_hash],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM paths WHERE path IN ('deleted-projection.rs', 'staging.rs')",
            [],
        )
        .unwrap();

    assert_eq!(symbol_names(&fixture.store, "target.rs"), ["old_target"]);
    assert_eq!(
        symbol_names(&fixture.store, "deleted-projection.rs"),
        ["stale_deleted"]
    );
    fixture.indexer.needs_rescan.store(true, Ordering::Release);
    fixture.indexer.process_pending_changes().await.unwrap();

    assert_eq!(symbol_names(&fixture.store, "target.rs"), ["new_target"]);
    assert!(symbol_names(&fixture.store, "deleted-projection.rs").is_empty());
    assert!(symbol_names(&fixture.store, "staging.rs").is_empty());
    assert_eq!(fixture.store.current().graph().paths(), ["target.rs"]);
    assert_eq!(fixture.store.current().searcher().num_docs(), 2);
    assert_eq!(
        fixture
            .store
            .current()
            .facts()
            .unwrap()
            .reader()
            .paths()
            .unwrap()
            .into_iter()
            .map(|row| row.path)
            .collect::<Vec<_>>(),
        ["target.rs"]
    );
}

#[tokio::test]
async fn watcher_delete_metadata_failure_preserves_path_until_reconciliation() {
    let fixture = fixture();
    let source = fixture.root.path().join("metadata-error.rs");
    std::fs::write(&source, "fn preserved_until_confirmed() {}\n").unwrap();
    enqueue(&fixture.indexer, source.clone()).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    std::fs::remove_file(&source).unwrap();
    fixture
        .indexer
        .fail_next_file_read_for_test("metadata-error.rs");
    enqueue_change(&fixture.indexer, source, FileChangeType::Deleted).await;
    fixture.indexer.process_pending_changes().await.unwrap();

    assert_eq!(
        symbol_names(&fixture.store, "metadata-error.rs"),
        ["preserved_until_confirmed"]
    );
    assert!(fixture.indexer.needs_rescan.load(Ordering::Acquire));

    fixture.indexer.process_pending_changes().await.unwrap();

    assert!(symbol_names(&fixture.store, "metadata-error.rs").is_empty());
    assert!(!fixture.indexer.needs_rescan.load(Ordering::Acquire));
}
