//! Tests for the file watcher module extracted from the implementation file to keep it lean.

use crate::language; // Centralized language support
use crate::watcher::IncrementalIndexer;
use crate::watcher::filtering;
use std::fs;

mod event_queue;

#[test]
fn test_supported_extensions() {
    let extensions = filtering::build_supported_extensions();

    // Test some key extensions
    assert!(extensions.contains("rs"));
    assert!(extensions.contains("ts"));
    assert!(extensions.contains("py"));
    assert!(extensions.contains("java"));
    assert!(
        extensions.contains("scala"),
        "Scala missing from supported extensions"
    );
    assert!(
        extensions.contains("sc"),
        "Scala .sc missing from supported extensions"
    );
    assert!(
        extensions.contains("ex"),
        "Elixir .ex missing from supported extensions"
    );
    assert!(
        extensions.contains("exs"),
        "Elixir .exs missing from supported extensions"
    );

    // Test that unsupported extensions are not included
    assert!(!extensions.contains("txt"));
    assert!(!extensions.contains("pdf"));
}

#[test]
fn test_ignore_patterns() {
    use std::fs;
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "**/node_modules/**\n*.log\n").unwrap();

    let gitignore = filtering::build_gitignore_matcher(dir.path()).unwrap();

    // Test that gitignore patterns work
    assert!(
        gitignore
            .matched_path_or_any_parents("src/node_modules/package.json", false)
            .is_ignore()
    );
    assert!(
        gitignore
            .matched_path_or_any_parents("frontend/node_modules/react/index.js", false)
            .is_ignore()
    );
}

#[test]
fn test_language_detection_by_extension() {
    let test_files = vec![
        ("test.rs", "rust"),
        ("app.ts", "typescript"),
        ("script.js", "javascript"),
        ("main.py", "python"),
        ("App.java", "java"),
        ("Program.cs", "csharp"),
        // Documentation and configuration languages (extractors #28-30)
        ("README.md", "markdown"),
        ("package.json", "json"),
        ("Cargo.toml", "toml"),
    ];

    for (filename, expected_lang) in test_files {
        // Extract extension from filename
        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .expect("Test file should have extension");

        let detected_lang = language::detect_language_from_extension(ext);
        assert_eq!(
            detected_lang,
            Some(expected_lang),
            "Failed to detect language for {}",
            filename
        );
    }
}

#[test]
fn test_watcher_filtering_keeps_text_only_and_extensionless_paths_in_sync() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workspace_root = temp_dir.path();
    let gitignore = filtering::build_gitignore_matcher(workspace_root).unwrap();
    let supported_extensions = filtering::build_supported_extensions();

    let extensionless = workspace_root.join("README");
    fs::write(&extensionless, "plain text\n").unwrap();
    assert!(
        filtering::should_index_file(
            &extensionless,
            &supported_extensions,
            &gitignore,
            workspace_root
        ),
        "extensionless text files should be indexed for watcher/discovery parity"
    );

    let text_only = workspace_root.join("notes.txt");
    fs::write(&text_only, "plain text\n").unwrap();
    assert!(
        filtering::should_index_file(
            &text_only,
            &supported_extensions,
            &gitignore,
            workspace_root
        ),
        "unsupported text extensions should be indexed for watcher/discovery parity"
    );

    fs::remove_file(&extensionless).unwrap();
    fs::remove_file(&text_only).unwrap();
    assert!(
        filtering::should_process_deletion(
            &extensionless,
            &supported_extensions,
            &gitignore,
            workspace_root
        ),
        "extensionless paths that were indexed must be deletable by watcher"
    );
    assert!(
        filtering::should_process_deletion(
            &text_only,
            &supported_extensions,
            &gitignore,
            workspace_root
        ),
        "text-only paths that were indexed must be deletable by watcher"
    );
}

#[tokio::test]
#[serial_test::serial]
#[ignore] // Flaky in test environment - file watcher events unreliable in parallel test runs
async fn test_real_time_file_watcher_indexing() {
    use crate::database::SymbolDatabase;
    use crate::extractors::ExtractorManager;
    use crate::tests::helpers::cleanup::atomic_cleanup_julie_dir;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::time::sleep;

    // CRITICAL: Use real directory instead of tmpfs (notify/inotify doesn't work reliably on tmpfs)
    // tempfile::TempDir uses /tmp which is often tmpfs on Linux, causing test failures
    let workspace_root = std::env::current_dir()
        .unwrap()
        .join(".test_watcher")
        .join(format!("test_{}", std::process::id()));

    // Cleanup BEFORE test
    atomic_cleanup_julie_dir(&workspace_root).ok(); // May not exist yet, ignore error

    fs::create_dir_all(&workspace_root).unwrap();

    // Initialize components
    let db_path = workspace_root.join(".julie/db/test.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));

    let cache_dir = workspace_root.join(".julie/cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let extractor_manager = Arc::new(ExtractorManager::new());

    // Create initial file to ensure workspace isn't empty
    let initial_file = workspace_root.join("initial.rs");
    fs::write(&initial_file, "fn initial() {}").unwrap();

    // Create and start watcher
    let shared_provider = std::sync::Arc::new(std::sync::RwLock::new(None));
    let mut indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        db.clone(),
        extractor_manager.clone(),
        None,
        shared_provider, // No embedding provider in test
        crate::tools::workspace::indexing::state::IndexingRuntimeState::shared(),
    )
    .unwrap();

    indexer.start_watching().await.unwrap();

    // Give watcher time to initialize
    sleep(Duration::from_millis(500)).await;

    // Create a new file AFTER watcher is running
    let test_file = workspace_root.join("new_file.rs");
    fs::write(&test_file, "fn test_function() { println!(\"hello\"); }").unwrap();
    eprintln!("✅ Created test file: {}", test_file.display());

    // Wait for file system event to be detected and queued
    // Note: Real filesystems (not tmpfs) may take longer for notify events
    sleep(Duration::from_millis(500)).await;

    // Check queue size before processing
    let queue_size = {
        let queue = indexer.index_queue.lock().await;
        queue.len()
    };
    eprintln!("📊 Queue has {} events before processing", queue_size);

    // TEMPORARY: Manually trigger processing to test if the processing logic works
    // (bypassing the background task timer)
    eprintln!("🔧 Manually triggering process_pending_changes()");
    indexer.process_pending_changes().await.unwrap();

    // Give a moment for processing to complete
    sleep(Duration::from_millis(500)).await;
    eprintln!("⏳ Done processing, checking database...");

    // Verify the new file is in the database
    let symbols = {
        let db_lock = db.lock().unwrap();
        db_lock.get_symbols_by_name("test_function").unwrap()
    };
    assert!(
        !symbols.is_empty(),
        "File watcher should have indexed new_file.rs without restart! Found {} symbols",
        symbols.len()
    );

    // Verify it's specifically from our new file
    let found = symbols
        .iter()
        .any(|s| s.file_path.contains("new_file.rs") && s.name == "test_function");
    assert!(
        found,
        "Should find test_function from new_file.rs. Symbols found: {:?}",
        symbols
    );

    // CLEANUP: Atomic cleanup AFTER test
    atomic_cleanup_julie_dir(&workspace_root).ok(); // Ignore errors if already removed
    fs::remove_dir_all(&workspace_root).ok(); // Clean up parent directory too
}

#[tokio::test]
async fn test_process_pending_changes_runs_rescan_repair_for_stale_and_new_files() {
    use crate::database::SymbolDatabase;
    use crate::extractors::ExtractorManager;
    use crate::watcher::handlers::handle_file_created_or_modified_static;
    use crate::workspace::mutation_gate::acquire_gate;
    use std::sync::{Arc, Mutex, atomic::Ordering};

    let temp_dir = crate::tests::helpers::unique_temp_dir("watcher_rescan_repair");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let tracked_file = workspace_root.join("tracked.rs");
    fs::write(&tracked_file, "fn before_rescan() {}\n").unwrap();

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
        crate::tools::workspace::indexing::state::IndexingRuntimeState::shared(),
    )
    .unwrap();

    let guard = acquire_gate("test_rescan_repair").await;
    handle_file_created_or_modified_static(
        tracked_file.canonicalize().unwrap(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("initial indexing should succeed");
    drop(guard); // release gate before process_pending_changes acquires its own

    fs::write(&tracked_file, "fn after_rescan() {}\n").unwrap();
    let new_file = workspace_root.join("fresh.rs");
    fs::write(&new_file, "fn discovered_during_rescan() {}\n").unwrap();

    indexer.needs_rescan.store(true, Ordering::Release);

    indexer
        .process_pending_changes()
        .await
        .expect("manual queue drain should run repair rescan too");

    assert!(
        !indexer.needs_rescan.load(Ordering::Acquire),
        "repair rescan should clear the overflow flag once it completes"
    );

    let db_lock = db.lock().unwrap();

    let tracked_symbols = db_lock.get_symbols_for_file("tracked.rs").unwrap();
    assert!(
        tracked_symbols
            .iter()
            .any(|symbol| symbol.name == "after_rescan"),
        "stale tracked file should be re-indexed during repair rescan"
    );
    assert!(
        tracked_symbols
            .iter()
            .all(|symbol| symbol.name != "before_rescan"),
        "repair rescan should replace stale tracked-file symbols"
    );

    let fresh_symbols = db_lock.get_symbols_for_file("fresh.rs").unwrap();
    assert!(
        fresh_symbols
            .iter()
            .any(|symbol| symbol.name == "discovered_during_rescan"),
        "repair rescan should discover files created while the watcher was behind"
    );
}

#[tokio::test]
async fn test_process_pending_changes_retries_persisted_extractor_failure() {
    use crate::database::SymbolDatabase;
    use crate::extractors::ExtractorManager;
    use std::sync::{Arc, Mutex};

    let temp_dir = crate::tests::helpers::unique_temp_dir("watcher_retry_persisted_repair");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let retry_file = workspace_root.join("retry.rs");
    fs::write(&retry_file, "fn retried_symbol() {}\n").unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let shared_provider = Arc::new(std::sync::RwLock::new(None));

    {
        let db_lock = db.lock().unwrap();
        db_lock
            .conn
            .execute(
                "INSERT INTO indexing_repairs (path, reason, detail, updated_at)
                 VALUES (?1, ?2, ?3, 0)",
                rusqlite::params!["retry.rs", "extractor_failure", "seeded retry"],
            )
            .expect("repair row should seed successfully");
    }

    let indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        db.clone(),
        extractor_manager,
        None,
        shared_provider,
        crate::tools::workspace::indexing::state::IndexingRuntimeState::shared(),
    )
    .unwrap();

    indexer
        .process_pending_changes()
        .await
        .expect("manual queue drain should retry persisted repairs");

    let db_lock = db.lock().unwrap();
    let symbols = db_lock.get_symbols_for_file("retry.rs").unwrap();
    assert!(
        symbols.iter().any(|symbol| symbol.name == "retried_symbol"),
        "persisted extractor repair should trigger a retry index pass"
    );

    let remaining_repairs: i64 = db_lock
        .conn
        .query_row(
            "SELECT COUNT(*) FROM indexing_repairs WHERE path = ?1",
            rusqlite::params!["retry.rs"],
            |row| row.get(0),
        )
        .expect("repair count query should succeed");
    assert_eq!(
        remaining_repairs, 0,
        "successful retry should clear the persisted repair entry"
    );
}

#[tokio::test]
async fn test_process_pending_changes_does_not_leave_watcher_repair_active_without_search_index() {
    use crate::database::SymbolDatabase;
    use crate::extractors::ExtractorManager;
    use crate::tools::workspace::indexing::state::{IndexingOperation, IndexingRuntimeState};
    use std::sync::{Arc, Mutex};

    let temp_dir = crate::tests::helpers::unique_temp_dir("watcher_dirty_without_search_index");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let shared_provider = Arc::new(std::sync::RwLock::new(None));
    let indexing_runtime = IndexingRuntimeState::shared();

    let indexer = IncrementalIndexer::new(
        workspace_root,
        db,
        extractor_manager,
        None,
        shared_provider,
        Arc::clone(&indexing_runtime),
    )
    .unwrap();

    indexer.mark_tantivy_dirty_for_test("src/stale.rs");

    indexer
        .process_pending_changes()
        .await
        .expect("queue drain should tolerate dirty Tantivy state without search index");

    let snapshot = indexing_runtime.read().unwrap().snapshot();
    assert_eq!(
        snapshot.active_operation, None,
        "missing search index must not leave watcher repair stuck active"
    );
    assert_eq!(
        snapshot.dirty_projection_count, 1,
        "dirty projection count should remain until Tantivy is available again"
    );
    assert!(
        snapshot.repair_reasons.contains(
            &crate::tools::workspace::indexing::state::IndexingRepairReason::TantivyDirty
        ),
        "dirty projection reason should stay visible for later repair"
    );
    assert_ne!(
        snapshot.active_operation,
        Some(IndexingOperation::WatcherRepair),
        "missing search index should not strand the runtime in watcher repair"
    );
}

#[tokio::test]
async fn test_run_guarded_task_step_returns_false_after_panic() {
    let completed = crate::watcher::run_guarded_task_step("panic-test", async move {
        panic!("boom");
    })
    .await;

    assert!(
        !completed,
        "guarded watcher steps should swallow panics and report failure"
    );
}

#[tokio::test]
async fn test_run_guarded_task_step_returns_true_after_success() {
    let completed = crate::watcher::run_guarded_task_step("success-test", async move {}).await;

    assert!(
        completed,
        "guarded watcher steps should report success when the step completes"
    );
}

/// Fix G: Real blake3 change detection test replacing the stub.
///
/// Verifies that handle_file_created_or_modified_static uses blake3 hashing
/// to skip re-indexing when file content hasn't changed, and re-indexes when
/// content does change.
#[tokio::test]
async fn test_blake3_change_detection() {
    use crate::database::SymbolDatabase;
    use crate::extractors::ExtractorManager;
    use crate::watcher::handlers::handle_file_created_or_modified_static;
    use crate::workspace::mutation_gate::acquire_gate;
    use std::sync::{Arc, Mutex};

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_blake3_change_detection").await;

    // 1. Create a file and index it for the first time
    let test_file = dir.path().join("example.rs");
    fs::write(&test_file, "pub fn hello() -> &'static str { \"hello\" }").unwrap();

    handle_file_created_or_modified_static(
        test_file.clone(),
        &db,
        &extractor_manager,
        dir.path(),
        None,
        &guard,
    )
    .await
    .expect("First index should succeed");

    // Verify symbol was indexed
    let count_after_first = {
        let db_lock = db.lock().unwrap();
        db_lock.get_symbols_for_file("example.rs").unwrap().len()
    };
    assert!(
        count_after_first > 0,
        "Symbol should be indexed after first pass"
    );

    // 2. Index same content again — blake3 hash matches, so this should be a no-op
    // We can detect this by modifying the DB state and confirming it's unchanged after
    // the second call (the handler returns early on hash match without touching the DB)
    {
        let db_lock = db.lock().unwrap();
        // Tamper: clear symbols to detect whether a re-index happens
        db_lock
            .conn
            .execute(
                "UPDATE files SET symbol_count = 999 WHERE path = 'example.rs'",
                [],
            )
            .unwrap();
    }

    handle_file_created_or_modified_static(
        test_file.clone(),
        &db,
        &extractor_manager,
        dir.path(),
        None,
        &guard,
    )
    .await
    .expect("Second index (same content) should succeed");

    // Tampered value should survive — handler skipped due to hash match
    let tampered_count = {
        let db_lock = db.lock().unwrap();
        let row: i64 = db_lock
            .conn
            .query_row(
                "SELECT symbol_count FROM files WHERE path = 'example.rs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        row
    };
    assert_eq!(
        tampered_count, 999,
        "Same content: handler should have skipped (blake3 match)"
    );

    // 3. Write different content — hash changes, should trigger re-index
    fs::write(
        &test_file,
        "pub fn goodbye() -> &'static str { \"goodbye\" }",
    )
    .unwrap();

    handle_file_created_or_modified_static(
        test_file.clone(),
        &db,
        &extractor_manager,
        dir.path(),
        None,
        &guard,
    )
    .await
    .expect("Third index (new content) should succeed");

    // Tampered value should be reset — handler re-indexed due to hash mismatch
    let reset_count = {
        let db_lock = db.lock().unwrap();
        let row: i64 = db_lock
            .conn
            .query_row(
                "SELECT symbol_count FROM files WHERE path = 'example.rs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        row
    };
    assert_ne!(
        reset_count, 999,
        "Different content: handler should have re-indexed (blake3 mismatch)"
    );

    // Verify new symbol exists
    let symbols = {
        let db_lock = db.lock().unwrap();
        db_lock.get_symbols_for_file("example.rs").unwrap()
    };
    assert!(
        symbols.iter().any(|s| s.name == "goodbye"),
        "New symbol 'goodbye' should be indexed after content change"
    );
}

/// Regression test: repair retry must skip and clear entries for unsupported
/// file extensions (e.g., binary audio/video files).
///
/// Bug: `.ogg` files from initial indexing ended up in the `indexing_repairs`
/// table with `extractor_failure`. `retry_persisted_repairs` dispatched them
/// every cycle without checking whether the extension is supported, causing
/// an infinite 1-second retry loop.
#[tokio::test]
async fn test_repair_retry_clears_unsupported_extension() {
    use crate::database::SymbolDatabase;
    use crate::extractors::ExtractorManager;
    use std::sync::{Arc, Mutex};

    let temp_dir = crate::tests::helpers::unique_temp_dir("watcher_repair_unsupported_ext");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    // Create a binary file that has an unsupported extension
    let ogg_file = workspace_root.join("audio.ogg");
    fs::write(&ogg_file, b"\x00\x01\x02binary content").unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let shared_provider = Arc::new(std::sync::RwLock::new(None));

    // Seed a repair entry for the .ogg file (simulates initial indexing failure)
    {
        let db_lock = db.lock().unwrap();
        db_lock
            .conn
            .execute(
                "INSERT INTO indexing_repairs (path, reason, detail, updated_at)
                 VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![
                    "audio.ogg",
                    "extractor_failure",
                    "stream did not contain valid UTF-8"
                ],
            )
            .expect("repair row should seed successfully");
    }

    let indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        db.clone(),
        extractor_manager,
        None,
        shared_provider,
        crate::tools::workspace::indexing::state::IndexingRuntimeState::shared(),
    )
    .unwrap();

    indexer
        .process_pending_changes()
        .await
        .expect("pending changes should handle unsupported extensions gracefully");

    // The repair entry for the .ogg file should be cleared, not retried
    let db_lock = db.lock().unwrap();
    let remaining: i64 = db_lock
        .conn
        .query_row(
            "SELECT COUNT(*) FROM indexing_repairs WHERE path = ?1",
            rusqlite::params!["audio.ogg"],
            |row| row.get(0),
        )
        .expect("repair count query should succeed");
    assert_eq!(
        remaining, 0,
        "repair entry for unsupported extension must be cleared, not retried forever"
    );
}
