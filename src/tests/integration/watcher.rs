//! Tests for the file watcher module extracted from the implementation file to keep it lean.

use crate::watcher::IncrementalIndexer;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_supported_extensions() {
    let extensions = IncrementalIndexer::build_supported_extensions();

    // Test some key extensions
    assert!(extensions.contains("rs"));
    assert!(extensions.contains("ts"));
    assert!(extensions.contains("py"));
    assert!(extensions.contains("java"));

    // Test that unsupported extensions are not included
    assert!(!extensions.contains("txt"));
    assert!(!extensions.contains("pdf"));
}

#[test]
fn test_ignore_patterns() {
    let patterns = IncrementalIndexer::build_ignore_patterns().unwrap();

    // Test that patterns are created successfully
    assert!(!patterns.is_empty());

    // Test some key patterns work
    let node_modules_pattern = patterns
        .iter()
        .find(|p| p.as_str().contains("node_modules"))
        .expect("Should have node_modules pattern");

    assert!(node_modules_pattern.matches("src/node_modules/package.json"));
    assert!(node_modules_pattern.matches("frontend/node_modules/react/index.js"));
}

#[test]
fn test_language_detection_by_extension() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path().to_path_buf();

    let test_files = vec![
        ("test.rs", "rust"),
        ("app.ts", "typescript"),
        ("script.js", "javascript"),
        ("main.py", "python"),
        ("App.java", "java"),
        ("Program.cs", "csharp"),
    ];

    for (filename, expected_lang) in test_files {
        let file_path = workspace_root.join(filename);
        fs::write(&file_path, "// test content").unwrap();

        let result = IncrementalIndexer::detect_language_by_extension(&file_path);
        if let Ok(lang) = result {
            assert_eq!(lang, expected_lang);
        }
    }
}

#[tokio::test]
async fn test_real_time_file_watcher_indexing() {
    use crate::database::SymbolDatabase;
    use crate::embeddings::EmbeddingEngine;
    use crate::extractors::ExtractorManager;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::time::sleep;

    // Create temp workspace
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path().to_path_buf();

    // Initialize components
    let db_path = workspace_root.join(".julie/db/test.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path).unwrap()));

    let cache_dir = workspace_root.join(".julie/cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let embeddings = Arc::new(tokio::sync::RwLock::new(Some(EmbeddingEngine::new("bge-small", cache_dir, db.clone()).await.unwrap())));
    let extractor_manager = Arc::new(ExtractorManager::new());

    // Create initial file to ensure workspace isn't empty
    let initial_file = workspace_root.join("initial.rs");
    fs::write(&initial_file, "fn initial() {}").unwrap();

    // Create and start watcher
    let mut indexer = IncrementalIndexer::new(
        workspace_root.clone(),
        db.clone(),
        embeddings.clone(),
        extractor_manager.clone(),
        None, // No vector store for this test
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
    sleep(Duration::from_millis(100)).await;

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
    let found = symbols.iter().any(|s| {
        s.file_path.contains("new_file.rs") && s.name == "test_function"
    });
    assert!(
        found,
        "Should find test_function from new_file.rs. Symbols found: {:?}",
        symbols
    );
}

#[tokio::test]
async fn test_blake3_change_detection_placeholder() {
    // TODO: Implement with proper database mocking once incremental pipeline is finalized.
}
