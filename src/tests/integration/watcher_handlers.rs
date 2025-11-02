//! Tests for incremental indexing file change handlers
//!
//! These tests verify that file creation, modification, deletion, and rename
//! operations correctly update the database with proper path handling.

use crate::database::SymbolDatabase;
use crate::embeddings::EmbeddingEngine;
use crate::extractors::ExtractorManager;
use crate::watcher::handlers::{
    handle_file_created_or_modified_static, handle_file_deleted_static,
    handle_file_renamed_static,
};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio::sync::RwLock;

/// Regression test for Bug: Incremental indexing path mismatch causes duplicate symbols
///
/// Bug: handle_file_created_or_modified_static receives absolute paths from the
/// file watcher, but uses them directly for database operations. However,
/// create_file_info() normalizes paths to relative Unix-style format.
///
/// Root cause:
/// - Line 36: `let path_str = path.to_string_lossy()` → absolute path
/// - Lines 45, 87, 110, 117: All use absolute path for DB operations
/// - Line 103: `create_file_info(&path, ...)` → stores relative path
///
/// Result:
/// - Blake3 hash check always fails (no match found)
/// - Old symbols NEVER deleted (duplicate symbols accumulate)
/// - Hash updates go to wrong key (ghost records)
/// - Database corruption grows with every file edit
///
/// Fix: Convert all paths to relative Unix-style before database operations.
#[tokio::test]
async fn test_incremental_indexing_absolute_path_handling() {
    // Skip background embedding tasks
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();

    // Create initial test file with one function
    let test_file = workspace_root.join("test.rs");
    let initial_content = r#"
fn initial_function() {
    println!("initial");
}
"#;
    fs::write(&test_file, initial_content).unwrap();

    // Get ABSOLUTE path (what the watcher provides)
    let absolute_path = test_file.canonicalize().unwrap();

    // Initialize database (use in-memory for test)
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(":memory:").expect("Failed to create test database"),
    ));

    // Initialize extractor manager
    let extractor_manager = Arc::new(ExtractorManager::new());

    // Initialize embeddings (None for this test)
    let embeddings = Arc::new(RwLock::new(None::<EmbeddingEngine>));

    println!("DEBUG: absolute_path = {}", absolute_path.display());
    println!("DEBUG: workspace_root = {}", workspace_root.display());

    // STEP 1: Index initial file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &embeddings,
        &extractor_manager,
        None,
        workspace_root,
    )
    .await
    .expect("Initial indexing should succeed");

    // Verify initial state: Should have 1 symbol
    {
        let db_lock = db.lock().unwrap();
        let symbol_count = db_lock
            .count_symbols_for_workspace()
            .expect("Failed to count symbols");
        assert_eq!(
            symbol_count, 1,
            "Should have 1 symbol after initial indexing"
        );

        // Verify symbol name
        let relative_path = "test.rs";
        let symbols = db_lock
            .get_symbols_for_file(relative_path)
            .expect("Failed to get symbols");
        assert_eq!(symbols.len(), 1, "Should have 1 symbol for test.rs");
        println!("DEBUG: Symbol file_path = {}", symbols[0].file_path);
        assert_eq!(
            symbols[0].name, "initial_function",
            "Symbol should be initial_function"
        );
    }

    // STEP 2: Modify file content (change function name)
    let modified_content = r#"
fn modified_function() {
    println!("modified");
}
"#;
    fs::write(&test_file, modified_content).unwrap();

    // STEP 3: Call handler with ABSOLUTE path (simulating watcher event)
    // THIS IS WHERE THE BUG OCCURS
    println!("\nDEBUG: About to call handler for MODIFICATION");
    println!("DEBUG: Using absolute path: {}", absolute_path.display());

    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &embeddings,
        &extractor_manager,
        None,
        workspace_root,
    )
    .await
    .expect("Incremental indexing should succeed");

    println!("DEBUG: Handler completed for modification");

    // STEP 4: Verify the bug - old symbols should be DELETED, not duplicated
    {
        let db_lock = db.lock().unwrap();
        let symbol_count = db_lock
            .count_symbols_for_workspace()
            .expect("Failed to count symbols");

        println!("\nDEBUG: After first modification, total symbol count = {}", symbol_count);

        // Query ALL symbols to see what's actually in the database
        let all_symbols = db_lock.conn.prepare("SELECT id, name, file_path FROM symbols")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        println!("DEBUG: All symbols in database:");
        for (id, name, file_path) in &all_symbols {
            println!("  - {} | {} | {}", id, name, file_path);
        }

        // BUG: Before fix, this will be 2 (both old and new symbols exist)
        // EXPECTED: After fix, this should be 1 (old deleted, new inserted)
        assert_eq!(
            symbol_count, 1,
            "Bug regression: Old symbols should be deleted before inserting new ones. \
             Found {} symbols (expected 1). This indicates duplicate symbols due to \
             path mismatch between absolute path operations and relative path storage.",
            symbol_count
        );

        // Verify we have the NEW symbol, not the old one
        let relative_path = "test.rs";
        let symbols = db_lock
            .get_symbols_for_file(relative_path)
            .expect("Failed to get symbols");
        assert_eq!(symbols.len(), 1, "Should have exactly 1 symbol for test.rs");
        assert_eq!(
            symbols[0].name, "modified_function",
            "Symbol should be modified_function, not initial_function"
        );

        // Verify NO symbols exist for the absolute path key (ghost records)
        let absolute_path_str = absolute_path.to_string_lossy();
        let ghost_symbols = db_lock
            .get_symbols_for_file(&absolute_path_str)
            .expect("Failed to query absolute path");
        assert_eq!(
            ghost_symbols.len(),
            0,
            "Bug regression: No symbols should be stored with absolute path key. \
             Found {} ghost symbols.",
            ghost_symbols.len()
        );
    }

    // STEP 4.5: Modify file AGAIN to expose accumulation
    let third_content = r#"
fn third_function() {
    println!("third");
}
"#;
    fs::write(&test_file, third_content).unwrap();

    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &embeddings,
        &extractor_manager,
        None,
        workspace_root,
    )
    .await
    .expect("Second modification should succeed");

    // STEP 4.6: Check for duplicate accumulation
    {
        let db_lock = db.lock().unwrap();
        let symbol_count = db_lock
            .count_symbols_for_workspace()
            .expect("Failed to count symbols");

        // BUG: Before fix, this will be 3 (all three functions exist!)
        // EXPECTED: After fix, this should be 1 (only latest function)
        assert_eq!(
            symbol_count, 1,
            "Bug regression: Duplicate symbols accumulating! \
             Found {} symbols after 2 modifications (expected 1). \
             This confirms path mismatch causes progressive corruption.",
            symbol_count
        );

        let relative_path = "test.rs";
        let symbols = db_lock
            .get_symbols_for_file(relative_path)
            .expect("Failed to get symbols");
        assert_eq!(symbols.len(), 1, "Should have exactly 1 symbol");
        assert_eq!(
            symbols[0].name, "third_function",
            "Should only have the latest function"
        );
    }

    // STEP 5: THE REAL BUG TEST - Blake3 hash check should work but DOESN'T
    // Rewrite the SAME content (same hash)
    println!("\nDEBUG: Testing Blake3 hash check with identical content");
    fs::write(&test_file, third_content).unwrap();

    // Add instrumentation to check if file was actually re-parsed
    let parse_count_before = {
        let db_lock = db.lock().unwrap();
        db_lock.count_symbols_for_workspace().unwrap()
    };

    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &embeddings,
        &extractor_manager,
        None,
        workspace_root,
    )
    .await
    .expect("Re-indexing unchanged file should succeed");

    // Symbol count should STILL be 1 (hash check prevented re-extraction)
    {
        let db_lock = db.lock().unwrap();
        let symbol_count = db_lock
            .count_symbols_for_workspace()
            .expect("Failed to count symbols");

        println!("DEBUG: Symbol count after rewrite: {}", symbol_count);

        assert_eq!(
            symbol_count, 1,
            "Blake3 hash check should prevent duplicate insertions"
        );
    }

    // STEP 6: Verify the FIX - hash check now works!
    // The handler now converts absolute paths to relative before database operations
    {
        let db_lock = db.lock().unwrap();

        // Hash should be stored with RELATIVE path key
        let relative_path = "test.rs";
        let hash_with_relative = db_lock.get_file_hash(relative_path).unwrap();

        println!("\nDEBUG: Hash stored with relative path: {:?}", hash_with_relative);

        assert!(
            hash_with_relative.is_some(),
            "File hash should be stored with relative path key"
        );

        // The handler has now been FIXED to convert absolute → relative
        // So hash checks work correctly (see step 5 - no re-indexing occurred)
    }

    // STEP 7: The REAL test - modify with DIFFERENT content, verify it re-indexes
    let final_content = r#"
fn final_function() {
    println!("final");
}
"#;
    fs::write(&test_file, final_content).unwrap();

    println!("\nDEBUG: Testing modification with DIFFERENT content (should re-index)");

    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &embeddings,
        &extractor_manager,
        None,
        workspace_root,
    )
    .await
    .expect("Re-indexing with new content should succeed");

    // Should have new symbol now
    {
        let db_lock = db.lock().unwrap();
        let symbols = db_lock.get_symbols_for_file("test.rs").unwrap();
        assert_eq!(symbols.len(), 1, "Should have 1 symbol");
        assert_eq!(symbols[0].name, "final_function", "Should have updated to new function");
    }
}

/// Test file deletion with absolute path
#[tokio::test]
async fn test_file_deletion_absolute_path() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();

    // Create and index a file
    let test_file = workspace_root.join("delete_me.rs");
    fs::write(&test_file, "fn example() {}").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(":memory:").expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let embeddings = Arc::new(RwLock::new(None::<EmbeddingEngine>));

    // Index the file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &embeddings,
        &extractor_manager,
        None,
        workspace_root,
    )
    .await
    .expect("Initial indexing should succeed");

    // Verify symbol exists
    {
        let db_lock = db.lock().unwrap();
        let count = db_lock.count_symbols_for_workspace().unwrap();
        assert_eq!(count, 1, "Should have 1 symbol before deletion");
    }

    // Delete the file physically
    fs::remove_file(&test_file).unwrap();

    // Call deletion handler with absolute path
    handle_file_deleted_static(absolute_path, &db, None, workspace_root)
        .await
        .expect("File deletion should succeed");

    // Verify symbols are deleted
    {
        let db_lock = db.lock().unwrap();
        let count = db_lock.count_symbols_for_workspace().unwrap();
        assert_eq!(count, 0, "All symbols should be deleted");
    }
}

/// Test file rename with absolute paths
#[tokio::test]
async fn test_file_rename_absolute_paths() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();

    // Create original file
    let old_file = workspace_root.join("old_name.rs");
    fs::write(&old_file, "fn old_function() {}").unwrap();
    let old_absolute = old_file.canonicalize().unwrap();

    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(":memory:").expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let embeddings = Arc::new(RwLock::new(None::<EmbeddingEngine>));

    // Index original file
    handle_file_created_or_modified_static(
        old_absolute.clone(),
        &db,
        &embeddings,
        &extractor_manager,
        None,
        workspace_root,
    )
    .await
    .expect("Initial indexing should succeed");

    // Rename file
    let new_file = workspace_root.join("new_name.rs");
    fs::rename(&old_file, &new_file).unwrap();
    let new_absolute = new_file.canonicalize().unwrap();

    // Call rename handler
    handle_file_renamed_static(
        old_absolute,
        new_absolute.clone(),
        &db,
        &embeddings,
        &extractor_manager,
        None,
        workspace_root,
    )
    .await
    .expect("File rename should succeed");

    // Verify old file has no symbols
    {
        let db_lock = db.lock().unwrap();
        let old_symbols = db_lock.get_symbols_for_file("old_name.rs").unwrap();
        assert_eq!(old_symbols.len(), 0, "Old file should have no symbols");

        // Verify new file has symbols
        let new_symbols = db_lock.get_symbols_for_file("new_name.rs").unwrap();
        assert_eq!(new_symbols.len(), 1, "New file should have 1 symbol");
        assert_eq!(new_symbols[0].name, "old_function");
    }
}
