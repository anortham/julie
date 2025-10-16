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
async fn test_file_change_queue_placeholder() {
    // TODO: Implement with proper mocking of dependencies once queues are wired.
}

#[tokio::test]
async fn test_blake3_change_detection_placeholder() {
    // TODO: Implement with proper database mocking once incremental pipeline is finalized.
}
