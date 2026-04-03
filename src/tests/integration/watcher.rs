//! Tests for the file watcher module extracted from the implementation file to keep it lean.

use crate::language; // Centralized language support
use crate::watcher::IncrementalIndexer;
use crate::watcher::filtering;
use std::fs;
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
async fn test_blake3_change_detection_placeholder() {
    // TODO: Implement with proper database mocking once incremental pipeline is finalized.
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

    process_file_system_event(
        &extensions,
        &gitignore,
        temp_dir.path(),
        queue.clone(),
        event,
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
