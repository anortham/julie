/// Tests extracted from src/cli/parallel.rs
///
/// This module contains inline tests that were originally embedded in the ParallelExtractor
/// implementation. They test the discovery and extraction of symbols in parallel, as well as
/// the fix for the "Cannot start a runtime from within a runtime" bug that occurred when
/// extract_directory (async) tried to spawn nested tokio runtimes.
use crate::cli::parallel::{ExtractionConfig, ParallelExtractor};
use tempfile::tempdir;

#[test]
fn test_discover_files() {
    let dir = tempdir().unwrap();
    let test_file = dir.path().join("test.rs");
    std::fs::write(&test_file, "fn main() {}").unwrap();

    let config = ExtractionConfig::default();
    let extractor = ParallelExtractor::new(config);

    let files = extractor
        .discover_files(dir.path().to_str().unwrap())
        .unwrap();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_extract_file() {
    let dir = tempdir().unwrap();
    let test_file = dir.path().join("test.rs");
    std::fs::write(&test_file, "fn main() {}").unwrap();

    let config = ExtractionConfig::default();
    let extractor = ParallelExtractor::new(config);

    let symbols = extractor.extract_file(test_file.to_str().unwrap()).unwrap();

    assert!(!symbols.is_empty());
}

/// This test verifies the fix for the "Cannot start a runtime from within a runtime" bug.
///
/// The bug was:
/// 1. extract_directory was async (running in tokio runtime)
/// 2. Rayon parallel iterator called extract_file_sync
/// 3. extract_file_sync tried to create a NEW runtime with Runtime::new()
/// 4. PANIC: nested runtime creation
///
/// The fix:
/// - Made extract_symbols synchronous (it doesn't need to be async)
/// - Removed Runtime::new() from extract_file_sync
/// - Now works perfectly without any runtime nesting
#[test]
fn test_bulk_extraction_no_runtime_panic() {
    let dir = tempdir().unwrap();
    let test_file = dir.path().join("test.rs");
    std::fs::write(&test_file, "fn main() { println!(\"test\"); }").unwrap();

    // This config triggers the bulk code path with SQLite
    let db_path = dir.path().join("test.db");
    let config = ExtractionConfig {
        num_threads: 2,
        batch_size: 1,
        output_db: Some(db_path.to_string_lossy().to_string()),
    };

    let extractor = ParallelExtractor::new(config);

    // This should now work without panicking!
    let symbols = extractor
        .extract_directory(dir.path().to_str().unwrap())
        .unwrap();

    // Verify we actually extracted symbols
    assert!(
        !symbols.is_empty(),
        "Should have extracted at least one symbol"
    );
}
