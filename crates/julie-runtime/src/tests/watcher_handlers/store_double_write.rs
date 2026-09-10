use std::fs;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use julie_core::database::SymbolDatabase;
use julie_index::checkout_store::CheckoutStore;

use crate::watcher::{FileChangeEvent, FileChangeType, IncrementalIndexer};

fn symbol_names(store: &CheckoutStore, path: &str) -> Vec<String> {
    let snapshot = store.current();
    let graph = snapshot.graph();
    graph
        .symbols_in_path(path)
        .iter()
        .map(|id| graph.symbol(*id).name.clone())
        .collect()
}

async fn push(indexer: &IncrementalIndexer, path: std::path::PathBuf, change_type: FileChangeType) {
    indexer.index_queue.lock().await.push_back(FileChangeEvent {
        path,
        change_type,
        timestamp: SystemTime::now(),
    });
}

#[tokio::test]
async fn queue_processing_writes_the_checkout_store_beside_the_symbol_database() {
    let temp_dir = julie_test_support::unique_temp_dir("watcher_store_double_write");
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let file = workspace_root.join("double.rs");
    fs::write(&file, "fn double_write_symbol() {}\n").unwrap();

    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(workspace_root.join("test.db")).unwrap(),
    ));
    let store =
        Arc::new(CheckoutStore::open(&temp_dir.path().join("index"), &workspace_root).unwrap());
    let indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        Arc::clone(&db),
        None,
        Arc::new(std::sync::RwLock::new(None)),
        julie_core::indexing_state::IndexingRuntimeState::shared(),
    )
    .unwrap()
    .with_store(Some(Arc::clone(&store)));

    push(&indexer, file.clone(), FileChangeType::Created).await;
    indexer.process_pending_changes().await.unwrap();

    assert_eq!(symbol_names(&store, "double.rs"), ["double_write_symbol"]);
    assert_eq!(
        db.lock()
            .unwrap()
            .get_symbols_for_file("double.rs")
            .unwrap()[0]
            .name,
        "double_write_symbol"
    );

    let renamed = workspace_root.join("renamed.rs");
    fs::rename(&file, &renamed).unwrap();
    push(
        &indexer,
        renamed.clone(),
        FileChangeType::Renamed {
            from: file.clone(),
            to: renamed.clone(),
        },
    )
    .await;
    indexer.process_pending_changes().await.unwrap();

    assert_eq!(store.current().graph().paths(), ["renamed.rs"]);

    fs::remove_file(&renamed).unwrap();
    push(&indexer, renamed, FileChangeType::Deleted).await;
    indexer.process_pending_changes().await.unwrap();

    assert!(store.current().graph().paths().is_empty());
    assert_eq!(store.status().blob_count, 1);
}
