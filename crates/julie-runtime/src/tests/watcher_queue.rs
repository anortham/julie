use crate::watcher::IncrementalIndexer;
use crate::watcher::events::process_file_system_event;
use crate::watcher::filtering;
use crate::watcher::types::{FileChangeEvent, FileChangeType};
use ignore::gitignore::Gitignore;
use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
use notify::{Event, EventKind};
use std::collections::{HashSet, VecDeque};
use std::fs;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;
use tokio::sync::Mutex as TokioMutex;

fn rs_extensions() -> HashSet<String> {
    let mut extensions = HashSet::new();
    extensions.insert("rs".to_string());
    extensions
}

fn gitignore_for(path: &std::path::Path) -> Gitignore {
    filtering::build_gitignore_matcher(path).expect("gitignore matcher should build")
}

fn seed_extractor_repair(db: &julie_core::database::SymbolDatabase, path: &str) {
    db.conn
        .execute(
            "INSERT INTO indexing_repairs (path, reason, detail, updated_at)
             VALUES (?1, ?2, ?3, 0)",
            rusqlite::params![
                path,
                julie_core::indexing_state::IndexingRepairReason::ExtractorFailure
                    .as_str(),
                "seeded repair"
            ],
        )
        .expect("repair row should seed successfully");
}

#[tokio::test]
async fn test_queue_coalesces_latest_event_per_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let target_file = temp_dir.path().join("coalesce.rs");
    fs::write(&target_file, "fn v1() {}").unwrap();

    let queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));
    let needs_rescan = Arc::new(AtomicBool::new(false));
    let extensions = rs_extensions();
    let gitignore = gitignore_for(temp_dir.path());

    for i in 0..3 {
        fs::write(&target_file, format!("fn v{}() {{}}", i)).unwrap();
        let modify_event = Event {
            kind: EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            paths: vec![target_file.clone()],
            attrs: Default::default(),
        };
        process_file_system_event(
            &extensions,
            &gitignore,
            temp_dir.path(),
            queue.clone(),
            modify_event,
            &needs_rescan,
        )
        .await
        .unwrap();
    }

    let queue = queue.lock().await;
    assert_eq!(
        queue.len(),
        1,
        "multiple modify events for one path should coalesce to one queued item",
    );
    assert!(matches!(queue[0].change_type, FileChangeType::Modified));
}

#[tokio::test]
async fn test_queue_atomic_save_coalesces_delete_then_modify() {
    let temp_dir = tempfile::tempdir().unwrap();
    let target_file = temp_dir.path().join("atomic.rs");
    fs::write(&target_file, "fn start() {}").unwrap();
    let deleted_path = target_file.canonicalize().unwrap();
    fs::remove_file(&target_file).unwrap();
    fs::write(&target_file, "fn rewritten() {}").unwrap();

    let queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));
    let needs_rescan = Arc::new(AtomicBool::new(false));
    let extensions = rs_extensions();
    let gitignore = gitignore_for(temp_dir.path());

    let delete_event = Event {
        kind: EventKind::Remove(RemoveKind::File),
        paths: vec![deleted_path.clone()],
        attrs: Default::default(),
    };
    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        delete_event,
        &needs_rescan,
    )
    .await
    .unwrap();

    let modify_event = Event {
        kind: EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
        paths: vec![deleted_path.clone()],
        attrs: Default::default(),
    };
    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        modify_event,
        &needs_rescan,
    )
    .await
    .unwrap();

    let queue = queue.lock().await;
    assert_eq!(
        queue.len(),
        1,
        "delete+modify for same path should coalesce"
    );
    assert!(
        matches!(queue[0].change_type, FileChangeType::Modified),
        "deleted then recreated file should resolve to modified",
    );
}

#[tokio::test]
async fn test_queue_rename_then_modify_preserves_rename() {
    let temp_dir = tempfile::tempdir().unwrap();
    let from_path = temp_dir.path().join("before.rs");
    let to_path = temp_dir.path().join("after.rs");
    fs::write(&from_path, "fn old_name() {}").unwrap();
    fs::rename(&from_path, &to_path).unwrap();
    fs::write(&to_path, "fn new_name() {}").unwrap();

    let queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));
    let needs_rescan = Arc::new(AtomicBool::new(false));
    let extensions = rs_extensions();
    let gitignore = gitignore_for(temp_dir.path());

    let rename_event = Event {
        kind: EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
        paths: vec![from_path.clone(), to_path.clone()],
        attrs: Default::default(),
    };
    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        rename_event,
        &needs_rescan,
    )
    .await
    .unwrap();

    let modify_event = Event {
        kind: EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
        paths: vec![to_path.clone()],
        attrs: Default::default(),
    };
    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        modify_event,
        &needs_rescan,
    )
    .await
    .unwrap();

    let queue = queue.lock().await;
    assert_eq!(
        queue.len(),
        1,
        "rename+modify should coalesce on rename target"
    );
    match &queue[0].change_type {
        FileChangeType::Renamed { from, to } => {
            assert_eq!(from, &from_path);
            assert_eq!(to, &to_path);
        }
        other => panic!("expected renamed event, got {other:?}"),
    }
}

#[tokio::test]
async fn test_queue_overflow_drains_to_headroom() {
    let temp_dir = tempfile::tempdir().unwrap();
    let queue: Arc<TokioMutex<VecDeque<FileChangeEvent>>> =
        Arc::new(TokioMutex::new(VecDeque::new()));
    let needs_rescan = Arc::new(AtomicBool::new(false));
    let extensions = rs_extensions();
    let gitignore = gitignore_for(temp_dir.path());

    {
        let mut q = queue.lock().await;
        for i in 0..1000 {
            q.push_back(FileChangeEvent {
                path: temp_dir.path().join(format!("seed{i}.rs")),
                change_type: FileChangeType::Modified,
                timestamp: SystemTime::now(),
            });
        }
    }

    let overflow_file = temp_dir.path().join("overflow.rs");
    fs::write(&overflow_file, "fn overflow() {}").unwrap();
    let create_event = Event {
        kind: EventKind::Create(CreateKind::File),
        paths: vec![overflow_file.clone()],
        attrs: Default::default(),
    };

    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        create_event,
        &needs_rescan,
    )
    .await
    .unwrap();

    let q = queue.lock().await;
    assert_eq!(
        q.len(),
        751,
        "overflow should drain to headroom before enqueue"
    );
    assert_eq!(
        q.front().map(|event| event.path.clone()),
        Some(temp_dir.path().join("seed250.rs")),
        "drain should remove a chunk of oldest events (250) not one-by-one",
    );
    assert_eq!(
        q.back().map(|event| event.path.clone()),
        Some(overflow_file),
        "incoming event should be queued after drain",
    );
    assert!(
        needs_rescan.load(Ordering::Acquire),
        "overflow drain should request follow-up rescan",
    );
}

#[tokio::test]
async fn test_runtime_drops_recent_duplicates_and_processes_delete_and_rename() {
    use julie_core::database::SymbolDatabase;
    use julie_extractors::ExtractorManager;

    let temp_dir = julie_test_support::unique_temp_dir("watcher_runtime_duplicate_policy");
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let shared_provider = Arc::new(std::sync::RwLock::new(None));

    let indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        db.clone(),
        extractor_manager,
        None,
        shared_provider,
        julie_core::indexing_state::IndexingRuntimeState::shared(),
    )
    .unwrap();

    // Duplicate Modified event for file_a should be dropped (not re-queued),
    // and should not block file_b processing in the same tick.
    let file_a = workspace_root.join("file_a.rs");
    let file_b = workspace_root.join("file_b.rs");
    fs::write(&file_a, "fn file_a_symbol() {}\n").unwrap();
    fs::write(&file_b, "fn file_b_symbol() {}\n").unwrap();

    {
        let mut q = indexer.index_queue.lock().await;
        q.push_back(FileChangeEvent {
            path: file_a.canonicalize().unwrap(),
            change_type: FileChangeType::Modified,
            timestamp: SystemTime::now(),
        });
        q.push_back(FileChangeEvent {
            path: file_a.canonicalize().unwrap(),
            change_type: FileChangeType::Modified,
            timestamp: SystemTime::now(),
        });
        q.push_back(FileChangeEvent {
            path: file_b.canonicalize().unwrap(),
            change_type: FileChangeType::Modified,
            timestamp: SystemTime::now(),
        });
    }

    indexer.process_pending_changes().await.unwrap();

    {
        let q = indexer.index_queue.lock().await;
        assert!(
            q.is_empty(),
            "recent duplicate Modified should be dropped, not re-queued",
        );
    }

    {
        let db_lock = db.lock().unwrap();
        assert!(
            db_lock
                .get_symbols_for_file("file_a.rs")
                .unwrap()
                .iter()
                .any(|symbol| symbol.name == "file_a_symbol"),
            "file_a should still be processed once",
        );
        assert!(
            db_lock
                .get_symbols_for_file("file_b.rs")
                .unwrap()
                .iter()
                .any(|symbol| symbol.name == "file_b_symbol"),
            "file_b should process in the same tick as file_a duplicate drop",
        );
    }

    // Duplicate Created event for file_c should be dropped (not re-queued).
    let file_c = workspace_root.join("file_c.rs");
    fs::write(&file_c, "fn file_c_symbol() {}\n").unwrap();
    {
        let mut q = indexer.index_queue.lock().await;
        q.push_back(FileChangeEvent {
            path: file_c.canonicalize().unwrap(),
            change_type: FileChangeType::Created,
            timestamp: SystemTime::now(),
        });
        q.push_back(FileChangeEvent {
            path: file_c.canonicalize().unwrap(),
            change_type: FileChangeType::Created,
            timestamp: SystemTime::now(),
        });
    }
    indexer.process_pending_changes().await.unwrap();
    assert!(
        indexer.index_queue.lock().await.is_empty(),
        "recent duplicate Created should be dropped, not re-queued",
    );

    // Deleted must still be processed even if this path was just processed.
    let file_d = workspace_root.join("file_d.rs");
    fs::write(&file_d, "fn file_d_old() {}\n").unwrap();
    {
        let mut q = indexer.index_queue.lock().await;
        q.push_back(FileChangeEvent {
            path: file_d.canonicalize().unwrap(),
            change_type: FileChangeType::Modified,
            timestamp: SystemTime::now(),
        });
    }
    indexer.process_pending_changes().await.unwrap();
    fs::remove_file(&file_d).unwrap();
    {
        let mut q = indexer.index_queue.lock().await;
        q.push_back(FileChangeEvent {
            path: workspace_root.join("file_d.rs"),
            change_type: FileChangeType::Deleted,
            timestamp: SystemTime::now(),
        });
    }
    indexer.process_pending_changes().await.unwrap();
    {
        let db_lock = db.lock().unwrap();
        assert_eq!(
            db_lock.get_symbols_for_file("file_d.rs").unwrap().len(),
            0,
            "Deleted event should process even when path was recently processed",
        );
    }
    assert!(
        indexer.index_queue.lock().await.is_empty(),
        "Deleted event should not be re-queued as duplicate",
    );

    // Renamed must still be processed even if rename target was just processed.
    let rename_from = workspace_root.join("rename_old.rs");
    let rename_to = workspace_root.join("rename_new.rs");
    fs::write(&rename_from, "fn rename_old_symbol() {}\n").unwrap();
    {
        let mut q = indexer.index_queue.lock().await;
        q.push_back(FileChangeEvent {
            path: rename_from.canonicalize().unwrap(),
            change_type: FileChangeType::Created,
            timestamp: SystemTime::now(),
        });
    }
    indexer.process_pending_changes().await.unwrap();
    fs::rename(&rename_from, &rename_to).unwrap();
    fs::write(&rename_to, "fn rename_new_symbol() {}\n").unwrap();

    {
        let mut q = indexer.index_queue.lock().await;
        q.push_back(FileChangeEvent {
            path: rename_to.canonicalize().unwrap(),
            change_type: FileChangeType::Modified,
            timestamp: SystemTime::now(),
        });
    }
    indexer.process_pending_changes().await.unwrap();

    {
        let mut q = indexer.index_queue.lock().await;
        q.push_back(FileChangeEvent {
            path: rename_to.canonicalize().unwrap(),
            change_type: FileChangeType::Renamed {
                from: rename_from.clone(),
                to: rename_to.canonicalize().unwrap(),
            },
            timestamp: SystemTime::now(),
        });
    }
    indexer.process_pending_changes().await.unwrap();

    {
        let db_lock = db.lock().unwrap();
        assert_eq!(
            db_lock.get_symbols_for_file("rename_old.rs").unwrap().len(),
            0,
            "Renamed event should retire source path even when target was recently processed",
        );
        assert!(
            db_lock
                .get_symbols_for_file("rename_new.rs")
                .unwrap()
                .iter()
                .any(|symbol| symbol.name == "rename_new_symbol"),
            "Renamed target should remain indexed",
        );
    }
    assert!(
        indexer.index_queue.lock().await.is_empty(),
        "Renamed event should not be re-queued as duplicate",
    );
}

#[tokio::test]
async fn test_overflow_repair_skips_unchanged_indexed_files() {
    use julie_core::database::SymbolDatabase;
    use julie_extractors::ExtractorManager;
    use julie_core::indexing_state::IndexingRepairReason;
    use crate::watcher::handlers::handle_file_created_or_modified_static;
    use crate::workspace::mutation_gate::acquire_gate;

    let temp_dir = julie_test_support::unique_temp_dir("watcher_repair_skip_unchanged");
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let shared_provider = Arc::new(std::sync::RwLock::new(None));

    let indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        db.clone(),
        extractor_manager.clone(),
        None,
        shared_provider,
        julie_core::indexing_state::IndexingRuntimeState::shared(),
    )
    .unwrap();

    let stable_file = workspace_root.join("stable.rs");
    fs::write(&stable_file, "fn stable_symbol() {}\n").unwrap();
    {
        let guard = acquire_gate("test_overflow_repair_skip_unchanged").await;
        handle_file_created_or_modified_static(
            stable_file.canonicalize().unwrap(),
            &db,
            &extractor_manager,
            &workspace_root,
            None,
            &guard,
        )
        .await
        .unwrap();
    } // guard dropped before process_pending_changes acquires its own

    {
        let db_lock = db.lock().unwrap();
        db_lock
            .record_indexing_repair(
                "stable.rs",
                IndexingRepairReason::DeletedFiles.as_str(),
                Some("sentinel"),
            )
            .unwrap();
    }

    indexer.needs_rescan.store(true, Ordering::Release);
    indexer.process_pending_changes().await.unwrap();

    let repairs = {
        let db_lock = db.lock().unwrap();
        db_lock.list_indexing_repairs().unwrap()
    };

    assert!(
        repairs.iter().any(|repair| repair.path == "stable.rs"),
        "unchanged indexed file should be skipped by overflow repair scan (no redispatch)",
    );
}

#[tokio::test]
async fn test_stop_with_pending_queue_does_not_wait_forever_when_gate_is_held() {
    use julie_core::database::SymbolDatabase;
    use julie_extractors::ExtractorManager;
    use crate::workspace::mutation_gate::Registry as MutationGateRegistry;
    use std::time::Duration;

    let temp_dir = julie_test_support::unique_temp_dir("watcher_stop_gate_held");
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let queued_file = workspace_root.join("queued.rs");
    fs::write(&queued_file, "fn queued_symbol() {}\n").unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let shared_provider = Arc::new(std::sync::RwLock::new(None));
    let mutation_gate_registry = Arc::new(MutationGateRegistry::new());
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_root.to_string_lossy())
            .unwrap();

    let mut indexer = IncrementalIndexer::new_with_mutation_gate_registry(
        workspace_root.clone(),
        db,
        extractor_manager,
        None,
        shared_provider,
        julie_core::indexing_state::IndexingRuntimeState::shared(),
        Arc::clone(&mutation_gate_registry),
    )
    .unwrap();
    indexer.start_watching().await.unwrap();

    indexer.index_queue.lock().await.push_back(FileChangeEvent {
        path: queued_file,
        change_type: FileChangeType::Modified,
        timestamp: SystemTime::now(),
    });

    let _gate = mutation_gate_registry.acquire(&workspace_id).await;
    let stopped = tokio::time::timeout(Duration::from_secs(5), indexer.stop()).await;

    assert!(
        stopped.is_ok(),
        "watcher stop should not wait forever for a mutation gate already held by its caller"
    );
    assert!(
        indexer.needs_rescan.load(Ordering::Acquire),
        "skipped pending watcher work should leave a rescan marker instead of disappearing silently"
    );
}

#[tokio::test]
async fn test_overflow_repair_processes_changed_deleted_new_supported_and_text_only() {
    use julie_core::database::SymbolDatabase;
    use julie_extractors::ExtractorManager;
    use crate::watcher::handlers::handle_file_created_or_modified_static;
    use crate::workspace::mutation_gate::acquire_gate;

    let temp_dir = julie_test_support::unique_temp_dir("watcher_repair_targeted_dispatch");
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let shared_provider = Arc::new(std::sync::RwLock::new(None));

    let indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        db.clone(),
        extractor_manager.clone(),
        None,
        shared_provider,
        julie_core::indexing_state::IndexingRuntimeState::shared(),
    )
    .unwrap();

    let unchanged = workspace_root.join("unchanged.rs");
    let changed = workspace_root.join("changed.rs");
    let deleted = workspace_root.join("deleted.rs");
    fs::write(&unchanged, "fn unchanged_symbol() {}\n").unwrap();
    fs::write(&changed, "fn before_change() {}\n").unwrap();
    fs::write(&deleted, "fn deleted_symbol() {}\n").unwrap();

    {
        let guard = acquire_gate("test_overflow_repair_targeted").await;
        for file in [&unchanged, &changed, &deleted] {
            handle_file_created_or_modified_static(
                file.canonicalize().unwrap(),
                &db,
                &extractor_manager,
                &workspace_root,
                None,
                &guard,
            )
            .await
            .unwrap();
        }
    } // guard dropped before process_pending_changes acquires its own

    fs::write(&changed, "fn after_change() {}\n").unwrap();
    fs::remove_file(&deleted).unwrap();

    let new_supported = workspace_root.join("fresh.rs");
    fs::write(&new_supported, "fn fresh_symbol() {}\n").unwrap();
    fs::write(workspace_root.join(".dockerignore"), "target/\n").unwrap();
    fs::write(workspace_root.join("flake.nix"), "{ }\n").unwrap();
    fs::write(
        workspace_root.join("pnpm-lock.yaml"),
        "lockfileVersion: '9.0'\n",
    )
    .unwrap();

    indexer.needs_rescan.store(true, Ordering::Release);
    indexer.process_pending_changes().await.unwrap();

    let (changed_symbols, deleted_symbols, fresh_symbols, indexed_files, repairs) = {
        let db_lock = db.lock().unwrap();
        (
            db_lock.get_symbols_for_file("changed.rs").unwrap(),
            db_lock.get_symbols_for_file("deleted.rs").unwrap(),
            db_lock.get_symbols_for_file("fresh.rs").unwrap(),
            db_lock.get_all_indexed_files().unwrap(),
            db_lock.list_indexing_repairs().unwrap(),
        )
    };

    assert!(
        changed_symbols
            .iter()
            .any(|symbol| symbol.name == "after_change"),
        "repair scan should re-index changed indexed files",
    );
    assert!(
        changed_symbols
            .iter()
            .all(|symbol| symbol.name != "before_change"),
        "repair scan should replace stale symbols for changed files",
    );
    assert!(
        deleted_symbols.is_empty(),
        "repair scan should remove symbols for missing indexed files",
    );
    assert!(
        fresh_symbols
            .iter()
            .any(|symbol| symbol.name == "fresh_symbol"),
        "repair scan should index newly discovered supported files",
    );
    assert!(
        indexed_files.contains(&".dockerignore".to_string()),
        "extensionless text files should be indexed as text-only during repair",
    );
    assert!(
        indexed_files.contains(&"flake.nix".to_string()),
        "unsupported textual extensions should be indexed as text-only during repair",
    );
    assert!(
        !indexed_files.contains(&"pnpm-lock.yaml".to_string()),
        "blacklisted filenames must not be dispatched as Created during repair",
    );
    assert!(
        !repairs.iter().any(|repair| repair.path == ".dockerignore"),
        "repair scan must not create extractor-failure repairs for text-only .dockerignore",
    );
    assert!(
        !repairs.iter().any(|repair| repair.path == "flake.nix"),
        "repair scan must not create extractor-failure repairs for text-only .nix files",
    );
    assert!(
        !repairs.iter().any(|repair| repair.path == "pnpm-lock.yaml"),
        "repair scan must not create repairs for blacklisted files",
    );
}

#[tokio::test]
async fn test_repair_retry_clears_unsupported_extensionless_and_unsupported_names() {
    use julie_core::database::SymbolDatabase;
    use julie_extractors::ExtractorManager;

    let temp_dir = julie_test_support::unique_temp_dir("watcher_retry_unsupported_names");
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let shared_provider = Arc::new(std::sync::RwLock::new(None));

    fs::write(workspace_root.join(".dockerignore"), "target/\n").unwrap();
    fs::write(workspace_root.join("Dockerfile"), "FROM scratch\n").unwrap();
    fs::write(workspace_root.join("flake.nix"), "{ }\n").unwrap();

    {
        let db_lock = db.lock().unwrap();
        seed_extractor_repair(&db_lock, ".dockerignore");
        seed_extractor_repair(&db_lock, "Dockerfile");
        seed_extractor_repair(&db_lock, "flake.nix");
    }

    let indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        db.clone(),
        extractor_manager,
        None,
        shared_provider,
        julie_core::indexing_state::IndexingRuntimeState::shared(),
    )
    .unwrap();

    indexer.process_pending_changes().await.unwrap();

    let remaining_paths: Vec<String> = {
        let db_lock = db.lock().unwrap();
        db_lock
            .list_indexing_repairs()
            .unwrap()
            .into_iter()
            .map(|repair| repair.path)
            .collect()
    };

    assert!(
        remaining_paths.is_empty(),
        "unsupported persisted repairs should be cleared instead of retried: {remaining_paths:?}",
    );
}

#[tokio::test]
async fn test_repair_retry_keeps_supported_extractor_failures_due_for_retry() {
    use julie_core::database::SymbolDatabase;
    use julie_extractors::ExtractorManager;
    use crate::watcher::handlers::handle_file_created_or_modified_static;
    use crate::workspace::mutation_gate::acquire_gate;

    let temp_dir = julie_test_support::unique_temp_dir("watcher_retry_supported_failure");
    let workspace_root = temp_dir.path().canonicalize().unwrap();
    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let shared_provider = Arc::new(std::sync::RwLock::new(None));

    let rust_file = workspace_root.join("broken.rs");
    fs::write(&rust_file, "fn previously_indexed() {}\n").unwrap();
    {
        let guard = acquire_gate("test_retry_supported_failure").await;
        handle_file_created_or_modified_static(
            rust_file.canonicalize().unwrap(),
            &db,
            &extractor_manager,
            &workspace_root,
            None,
            &guard,
        )
        .await
        .unwrap();
    } // guard dropped before process_pending_changes acquires its own

    fs::write(&rust_file, "// no symbols remain\n").unwrap();
    {
        let db_lock = db.lock().unwrap();
        seed_extractor_repair(&db_lock, "broken.rs");
    }

    let indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        db.clone(),
        extractor_manager,
        None,
        shared_provider,
        julie_core::indexing_state::IndexingRuntimeState::shared(),
    )
    .unwrap();

    indexer.process_pending_changes().await.unwrap();

    let repairs = {
        let db_lock = db.lock().unwrap();
        db_lock.list_indexing_repairs().unwrap()
    };

    assert!(
        repairs.iter().any(|repair| repair.path == "broken.rs"
            && repair
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("refused to drop"))),
        "supported files with extractor failures should remain repair candidates",
    );
}
