//! Tests for incremental indexing file change handlers
//!
//! These tests verify that file creation, modification, deletion, and rename
//! operations correctly update the database with proper path handling.

use crate::database::SymbolDatabase;
use crate::extractors::ExtractorManager;
use crate::tools::workspace::indexing::state::IndexingRepairReason;
use crate::watcher::handlers::{
    handle_file_created_or_modified_static, handle_file_deleted_static, handle_file_renamed_static,
};
use std::fs;
use std::sync::{Arc, Mutex};

/// Regression test for Bug: File watcher drops identifiers, types, and relationships
///
/// Bug: handle_file_created_or_modified_static only calls extract_symbols() and
/// stores symbols. It does NOT extract or store identifiers, types, or relationships.
///
/// Impact: fast_refs degrades over time as files are re-indexed by the watcher,
/// because identifiers (needed for reference tracking) are silently dropped.
#[tokio::test]
async fn test_incremental_indexing_stores_identifiers_and_relationships() {
    let temp_dir = crate::tests::helpers::unique_temp_dir("watcher_full_data");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    // Create a Rust file with function calls (produces identifiers + relationships)
    let test_file = workspace_root.join("test.rs");
    let code = r#"
fn helper() -> i32 {
    42
}

fn caller() -> i32 {
    helper()
}
"#;
    fs::write(&test_file, code).unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    // Initialize database
    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    // Index the file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
    )
    .await
    .expect("Indexing should succeed");

    // Verify symbols are stored (this already works)
    let db_lock = db.lock().unwrap();
    let symbols = db_lock.get_symbols_for_file("test.rs").unwrap();
    assert!(
        symbols.len() >= 2,
        "Should have at least 2 symbols (helper + caller), got {}",
        symbols.len()
    );

    // CRITICAL: Verify identifiers are stored (this is the bug)
    let identifier_count: i64 = db_lock
        .conn
        .query_row(
            "SELECT COUNT(*) FROM identifiers WHERE file_path = ?1",
            rusqlite::params!["test.rs"],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        identifier_count > 0,
        "BUG: No identifiers stored for test.rs! \
         The handler must extract and store identifiers for fast_refs to work. \
         Got {} identifiers, expected at least 1.",
        identifier_count
    );

    // Verify relationships are stored (caller -> helper)
    let relationship_count: i64 = db_lock
        .conn
        .query_row(
            "SELECT COUNT(*) FROM relationships WHERE file_path = ?1",
            rusqlite::params!["test.rs"],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        relationship_count > 0,
        "BUG: No relationships stored for test.rs! \
         The handler must extract and store relationships for trace_call_path to work. \
         Got {} relationships, expected at least 1.",
        relationship_count
    );
}

/// Regression test for Bug: Incremental indexing path mismatch causes duplicate symbols
///
/// Bug: handle_file_created_or_modified_static receives absolute paths from the
/// file watcher, but uses them directly for database operations. However,
/// create_file_info() normalizes paths to relative Unix-style format.
///
/// Root cause:
/// - Line 36: `let path_str = path.to_string_lossy()` -> absolute path
/// - Lines 45, 87, 110, 117: All use absolute path for DB operations
/// - Line 103: `create_file_info(&path, ...)` -> stores relative path
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
    let temp_dir = crate::tests::helpers::unique_temp_dir("incremental_indexing");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    // Create initial test file with one function
    // NOTE: Avoid macro invocations (e.g. println!) — the Rust extractor captures
    // them as symbols, inflating counts beyond the intended function-only assertions.
    let test_file = workspace_root.join("test.rs");
    let initial_content = r#"
fn initial_function() {
    let _ = 0;
}
"#;
    fs::write(&test_file, initial_content).unwrap();

    // Get ABSOLUTE path (what the watcher provides)
    let absolute_path = test_file.canonicalize().unwrap();

    // Initialize database (use file-based temp database for WAL support)
    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));

    // Initialize extractor manager
    let extractor_manager = Arc::new(ExtractorManager::new());

    println!("DEBUG: absolute_path = {}", absolute_path.display());
    println!("DEBUG: workspace_root = {}", workspace_root.display());

    // STEP 1: Index initial file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
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
    let _ = 1;
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
        &extractor_manager,
        &workspace_root,
        None,
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

        println!(
            "\nDEBUG: After first modification, total symbol count = {}",
            symbol_count
        );

        // Query ALL symbols to see what's actually in the database
        let all_symbols = db_lock
            .conn
            .prepare("SELECT id, name, file_path FROM symbols")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
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
    let _ = 2;
}
"#;
    fs::write(&test_file, third_content).unwrap();

    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
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
    let _parse_count_before = {
        let db_lock = db.lock().unwrap();
        db_lock.count_symbols_for_workspace().unwrap()
    };

    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
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

        println!(
            "\nDEBUG: Hash stored with relative path: {:?}",
            hash_with_relative
        );

        assert!(
            hash_with_relative.is_some(),
            "File hash should be stored with relative path key"
        );

        // The handler has now been FIXED to convert absolute -> relative
        // So hash checks work correctly (see step 5 - no re-indexing occurred)
    }

    // STEP 7: The REAL test - modify with DIFFERENT content, verify it re-indexes
    let final_content = r#"
fn final_function() {
    let _ = 3;
}
"#;
    fs::write(&test_file, final_content).unwrap();

    println!("\nDEBUG: Testing modification with DIFFERENT content (should re-index)");

    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
    )
    .await
    .expect("Re-indexing with new content should succeed");

    // Should have new symbol now
    {
        let db_lock = db.lock().unwrap();
        let symbols = db_lock.get_symbols_for_file("test.rs").unwrap();
        assert_eq!(symbols.len(), 1, "Should have 1 symbol");
        assert_eq!(
            symbols[0].name, "final_function",
            "Should have updated to new function"
        );
    }
}

/// Test file deletion with absolute path
#[tokio::test]
async fn test_file_deletion_absolute_path() {
    let temp_dir = crate::tests::helpers::unique_temp_dir("file_deletion");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    // Create and index a file
    let test_file = workspace_root.join("delete_me.rs");
    fs::write(&test_file, "fn example() {}").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    // Index the file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
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
    handle_file_deleted_static(absolute_path, &db, &workspace_root, None)
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
    let temp_dir = crate::tests::helpers::unique_temp_dir("file_rename");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    // Create original file
    let old_file = workspace_root.join("old_name.rs");
    fs::write(&old_file, "fn old_function() {}").unwrap();
    let old_absolute = old_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    // Index original file
    handle_file_created_or_modified_static(
        old_absolute.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
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
        &extractor_manager,
        &workspace_root,
        None,
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

/// Rename safety regression: if the destination re-index fails, the source
/// path must stay indexed instead of being deleted first.
#[tokio::test]
async fn test_file_rename_keeps_source_indexed_when_destination_reindex_fails() {
    let temp_dir = crate::tests::helpers::unique_temp_dir("file_rename_destination_failure");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let old_file = workspace_root.join("old_name.rs");
    fs::write(&old_file, "fn old_function() {}").unwrap();
    let old_absolute = old_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    handle_file_created_or_modified_static(
        old_absolute.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
    )
    .await
    .expect("Initial indexing should succeed");

    let new_file = workspace_root.join("new_name.txt");
    fs::rename(&old_file, &new_file).unwrap();
    let new_absolute = new_file.canonicalize().unwrap();

    handle_file_renamed_static(
        old_absolute,
        new_absolute,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
    )
    .await
    .expect("Rename handler should report the destination failure without panicking");

    let db_lock = db.lock().unwrap();
    let old_symbols = db_lock.get_symbols_for_file("old_name.rs").unwrap();
    assert_eq!(
        old_symbols.len(),
        1,
        "source path should remain indexed when destination reindex fails"
    );
    assert_eq!(old_symbols[0].name, "old_function");

    let new_symbols = db_lock.get_symbols_for_file("new_name.txt").unwrap();
    assert!(
        new_symbols.is_empty(),
        "failed destination should not replace the source index"
    );
}

#[tokio::test]
async fn test_file_rename_persists_repair_when_source_retirement_fails() {
    let temp_dir = crate::tests::helpers::unique_temp_dir("file_rename_source_retirement_failure");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let old_file = workspace_root.join("old_name.rs");
    fs::write(&old_file, "fn old_function() {}\n").unwrap();
    let old_absolute = old_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    handle_file_created_or_modified_static(
        old_absolute.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
    )
    .await
    .expect("Initial indexing should succeed");

    let new_file = workspace_root.join("new_name.rs");
    fs::rename(&old_file, &new_file).unwrap();
    let new_absolute = new_file.canonicalize().unwrap();

    {
        let db_lock = db.lock().unwrap();
        db_lock
            .conn
            .execute_batch(
                "CREATE TRIGGER fail_old_file_delete
                 BEFORE DELETE ON files
                 WHEN OLD.path = 'old_name.rs'
                 BEGIN
                   SELECT RAISE(FAIL, 'forced delete failure');
                 END;",
            )
            .expect("delete failure trigger should install");
    }

    let err = handle_file_renamed_static(
        old_absolute,
        new_absolute,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
    )
    .await
    .expect_err("source retirement failure should bubble up");

    assert!(
        err.to_string().contains("forced delete failure"),
        "rename failure should preserve the source delete error"
    );

    let db_lock = db.lock().unwrap();
    let new_symbols = db_lock.get_symbols_for_file("new_name.rs").unwrap();
    assert_eq!(
        new_symbols.len(),
        1,
        "destination index should still exist after source retirement failure"
    );

    let persisted = db_lock
        .conn
        .query_row(
            "SELECT reason FROM indexing_repairs WHERE path = ?1",
            rusqlite::params!["old_name.rs"],
            |row| row.get::<_, String>(0),
        )
        .expect("source retirement failure should persist a repair record");
    assert_eq!(persisted, "deleted_files");
}

#[tokio::test]
async fn test_extractor_failure_is_persisted_durably() {
    let temp_dir = crate::tests::helpers::unique_temp_dir("watcher_extractor_failure_repair");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let test_file = workspace_root.join("broken.txt");
    fs::write(&test_file, "plain text without a supported extractor\n").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    let outcome = handle_file_created_or_modified_static(
        absolute_path,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
    )
    .await
    .expect("Extractor failure should surface as repair-needed, not a hard error");

    assert_eq!(
        outcome.repair_reason,
        Some(IndexingRepairReason::ExtractorFailure),
        "unsupported extractor path should use the extractor-failure repair reason"
    );

    drop(db);

    let reopened = SymbolDatabase::new(&db_path).expect("Failed to reopen database");
    let persisted = reopened
        .conn
        .query_row(
            "SELECT reason, detail FROM indexing_repairs WHERE path = ?1",
            rusqlite::params!["broken.txt"],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .expect("repair state should persist across database reopen");

    assert_eq!(persisted.0, "extractor_failure");
    assert!(
        persisted
            .1
            .as_deref()
            .unwrap_or_default()
            .contains("Unsupported file extension"),
        "repair detail should preserve the extractor failure context"
    );
}

// test_transaction_leak_on_error was removed: the raw begin_transaction /
// rollback_transaction helpers were deleted in favour of conn.transaction()
// (RAII). Transaction leaks from that pattern are structurally impossible now.

/// Test: handle_file_deleted_static always cleans up regardless of disk state.
///
/// Fix B-a: The old path.exists() guard has been removed from handle_file_deleted_static.
/// The atomic-save check (skip if file still exists) now lives exclusively in
/// dispatch_file_event, which guards before calling this function. This eliminates
/// the TOCTOU window where embeddings could be deleted by the caller while symbols
/// survived because the file was recreated between the two independent checks.
///
/// The handler now trusts the caller's decision to proceed with deletion.
#[tokio::test]
async fn test_delete_handler_always_cleans_up() {
    let temp_dir = crate::tests::helpers::unique_temp_dir("atomic_save");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    // Create and index a file
    let test_file = workspace_root.join("edited_not_deleted.rs");
    fs::write(&test_file, "fn original() {}").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    // Index the file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
    )
    .await
    .expect("Initial indexing should succeed");

    // Verify symbol exists
    {
        let db_lock = db.lock().unwrap();
        let count = db_lock.count_symbols_for_workspace().unwrap();
        assert_eq!(count, 1, "Should have 1 symbol before deletion event");
    }

    // File still exists on disk — but we call handle_file_deleted_static directly,
    // simulating the scenario where the caller (dispatch_file_event) has already
    // decided to proceed with deletion. The handler must trust the caller.
    assert!(
        test_file.exists(),
        "File still exists — handler must clean up regardless (trust caller)"
    );

    handle_file_deleted_static(absolute_path, &db, &workspace_root, None)
        .await
        .expect("Delete handler should succeed");

    // Symbols MUST be deleted — the handler no longer has its own path.exists() guard
    {
        let db_lock = db.lock().unwrap();
        let count = db_lock.count_symbols_for_workspace().unwrap();
        assert_eq!(
            count, 0,
            "handle_file_deleted_static must clean up symbols regardless of disk state (Fix B-a)"
        );
    }
}

/// Regression test for Bug: File watcher drops Tantivy file content documents
///
/// Bug: handle_file_created_or_modified_static calls remove_by_file_path() which
/// deletes BOTH symbol docs AND file content docs from Tantivy, but only re-adds
/// symbol docs via add_symbol(). The add_file_content() call is missing.
///
/// Impact: Every file save progressively erodes the content search index. After
/// hours of editing, fast_search with search_target="content" returns zero results
/// for commonly-edited files because their content documents have been deleted.
///
/// Root cause:
/// - Line 164: idx.remove_by_file_path() — deletes ALL docs (symbols + file content)
/// - Lines 167-170: Only adds symbol docs back
/// - MISSING: idx.add_file_content() call to re-add file content doc
///
/// Compare with populate_tantivy_index() in processor.rs which correctly adds both.
#[tokio::test]
async fn test_incremental_indexing_preserves_tantivy_file_content() {
    use crate::search::index::{FileDocument, SearchFilter, SearchIndex};

    let temp_dir = crate::tests::helpers::unique_temp_dir("watcher_tantivy_content");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    // Create a Rust file with a distinctive identifier for content search
    let test_file = workspace_root.join("rich_component.rs");
    let initial_content = r#"
fn render_rich_text_field() {
    let widget = RichTextField::new();
    widget.display();
}
"#;
    fs::write(&test_file, initial_content).unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    // Initialize database
    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    // Create Tantivy search index
    let tantivy_dir = workspace_root.join("tantivy");
    fs::create_dir_all(&tantivy_dir).unwrap();
    let search_index = Arc::new(Mutex::new(
        SearchIndex::create(&tantivy_dir).expect("Failed to create search index"),
    ));

    // Seed Tantivy with initial file content (simulating what initial indexing does)
    {
        let idx = search_index.lock().unwrap();
        idx.add_file_content(&FileDocument {
            file_path: "rich_component.rs".into(),
            content: initial_content.into(),
            language: "rust".into(),
        })
        .unwrap();
        idx.commit().unwrap();
    }

    // Verify content search works BEFORE incremental update
    {
        let idx = search_index.lock().unwrap();
        let results = idx
            .search_content("RichTextField", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Content search should find 'RichTextField' before incremental update"
        );
    }

    // Now simulate a file modification via the watcher handler
    let modified_content = r#"
fn render_rich_text_field() {
    let widget = RichTextField::new();
    widget.set_value("hello");
    widget.display();
}
"#;
    fs::write(&test_file, modified_content).unwrap();

    // Call the watcher handler WITH the search index (this is the code path that has the bug)
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        Some(&search_index),
    )
    .await
    .expect("Incremental indexing should succeed");

    // The handler intentionally defers Tantivy commit (production caller
    // process_pending_changes batch-commits after processing all events).
    // We must commit here to make the changes visible to the reader.
    {
        let idx = search_index.lock().unwrap();
        idx.commit().unwrap();
    }

    // Verify content search STILL works after incremental update
    {
        let idx = search_index.lock().unwrap();
        let results = idx
            .search_content("RichTextField", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Content search for 'RichTextField' should still work after file modification"
        );
        assert_eq!(
            results[0].file_path, "rich_component.rs",
            "Content search should find the correct file"
        );
    }

    // Also verify the NEW content is searchable (not just the old content)
    {
        let idx = search_index.lock().unwrap();
        let results = idx
            .search_content("set_value", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Content search should find new content 'set_value' added in the modification"
        );
    }

    // Verify symbol search still works too (sanity check)
    {
        let idx = search_index.lock().unwrap();
        let results = idx
            .search_symbols("render_rich_text_field", &SearchFilter::default(), 10)
            .unwrap()
            .results;
        assert!(
            !results.is_empty(),
            "Symbol search should still find 'render_rich_text_field' after incremental update"
        );
    }
}

#[tokio::test]
async fn test_incremental_indexing_projection_failure_reports_repair_reason() {
    use crate::search::index::SearchIndex;

    let temp_dir = crate::tests::helpers::unique_temp_dir("watcher_projection_repair");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let test_file = workspace_root.join("projection_failure.rs");
    fs::write(&test_file, "fn projection_failure() {}\n").unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());

    let tantivy_dir = workspace_root.join("tantivy");
    fs::create_dir_all(&tantivy_dir).unwrap();
    let search_index = Arc::new(Mutex::new(
        SearchIndex::create(&tantivy_dir).expect("Failed to create search index"),
    ));
    {
        let idx = search_index.lock().unwrap();
        idx.shutdown()
            .expect("search index should shut down cleanly");
    }

    let outcome = handle_file_created_or_modified_static(
        absolute_path,
        &db,
        &extractor_manager,
        &workspace_root,
        Some(&search_index),
    )
    .await
    .expect("SQLite update should still succeed when projection fails");

    assert!(
        !outcome.tantivy_ok,
        "projection failure should surface a failed Tantivy status"
    );
    assert_eq!(
        outcome.repair_reason,
        Some(IndexingRepairReason::ProjectionFailure),
        "projection failure should use the shared repair vocabulary"
    );
}
