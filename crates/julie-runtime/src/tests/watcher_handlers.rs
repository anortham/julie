//! Tests for incremental indexing file change handlers
//!
//! These tests verify that file creation, modification, deletion, and rename
//! operations correctly update the database with proper path handling.

use crate::watcher::handlers::{
    handle_file_created_or_modified_static, handle_file_deleted_static, handle_file_renamed_static,
};
use crate::workspace::mutation_gate::acquire_gate;
use julie_core::database::SymbolDatabase;
use julie_core::indexing_state::IndexingRepairReason;
use julie_extractors::ExtractorManager;
use std::fs;
use std::sync::{Arc, Mutex};

mod enrichment_domains;
mod repair_projection;

/// Regression test for Bug: File watcher drops identifiers, types, and relationships
///
/// Bug: handle_file_created_or_modified_static only calls extract_symbols() and
/// stores symbols. It does NOT extract or store identifiers, types, or relationships.
///
/// Impact: fast_refs degrades over time as files are re-indexed by the watcher,
/// because identifiers (needed for reference tracking) are silently dropped.
#[tokio::test]
async fn test_incremental_indexing_stores_identifiers_and_relationships() {
    let temp_dir = julie_test_support::unique_temp_dir("watcher_full_data");
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
    let guard = acquire_gate("test_stores_identifiers").await;

    // Index the file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
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

#[tokio::test]
async fn test_incremental_indexing_stores_cpp_language_for_cpp_h_header() {
    let temp_dir = julie_test_support::unique_temp_dir("watcher_cpp_header_language");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let include_dir = workspace_root.join("include");
    fs::create_dir_all(&include_dir).unwrap();
    let test_file = include_dir.join("widget.h");
    fs::write(
        &test_file,
        r#"
#pragma once
namespace app {
class Widget {
public:
    int value() const { return 42; }
};
}
"#,
    )
    .unwrap();
    let absolute_path = test_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_watcher_cpp_header_language").await;

    handle_file_created_or_modified_static(
        absolute_path,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("watcher indexing should succeed");

    let db_lock = db.lock().unwrap();
    let stored_language: String = db_lock
        .conn
        .query_row(
            "SELECT language FROM files WHERE path = ?1",
            rusqlite::params!["include/widget.h"],
            |row| row.get(0),
        )
        .expect("watcher should store widget.h file metadata");
    assert_eq!(
        stored_language, "cpp",
        "watcher should store source-aware language for C++ .h headers"
    );

    let symbols = db_lock.get_symbols_for_file("include/widget.h").unwrap();
    assert!(
        symbols
            .iter()
            .any(|symbol| symbol.name == "Widget" && symbol.language == "cpp"),
        "watcher should store C++ symbols for .h header: {:?}",
        symbols
            .iter()
            .map(|symbol| (&symbol.name, &symbol.language))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_incremental_indexing_resolves_cross_file_pending_relationships() {
    let temp_dir = julie_test_support::unique_temp_dir("watcher_pending_resolution");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let caller_file = workspace_root.join("caller.rs");
    fs::write(
        &caller_file,
        r#"
fn caller() {}
"#,
    )
    .unwrap();
    let caller_abs = caller_file.canonicalize().unwrap();

    let callee_file = workspace_root.join("search").join("hybrid.rs");
    fs::create_dir_all(callee_file.parent().unwrap()).unwrap();
    fs::write(
        &callee_file,
        r#"
pub fn should_use_semantic_fallback() {}
"#,
    )
    .unwrap();
    let callee_abs = callee_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_cross_file_pending").await;

    handle_file_created_or_modified_static(
        callee_abs,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("callee file indexing should succeed");

    handle_file_created_or_modified_static(
        caller_abs.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("initial caller file indexing should succeed");

    fs::write(
        &caller_file,
        r#"
fn caller() {
    crate::search::hybrid::should_use_semantic_fallback();
}
"#,
    )
    .unwrap();

    handle_file_created_or_modified_static(
        caller_abs,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("caller update introducing cross-file call should succeed");

    let db_lock = db.lock().unwrap();
    let resolved_calls: i64 = db_lock
        .conn
        .query_row(
            "SELECT COUNT(*)
             FROM relationships rel
             INNER JOIN symbols src ON src.id = rel.from_symbol_id
             INNER JOIN symbols dst ON dst.id = rel.to_symbol_id
             WHERE src.name = 'caller'
               AND dst.name = 'should_use_semantic_fallback'
               AND rel.kind = 'calls'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(
        resolved_calls > 0,
        "watcher updates must resolve/store cross-file pending calls just like batch indexing"
    );
}

#[tokio::test]
async fn test_incremental_indexing_oversized_parser_file_switches_to_text_only_without_repair() {
    let temp_dir = julie_test_support::unique_temp_dir("watcher_oversized_text_only");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let file_path = workspace_root.join("main.rs");
    fs::write(
        &file_path,
        r#"
fn original_symbol() {}
"#,
    )
    .unwrap();
    let absolute_path = file_path.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_oversized_text_only").await;

    let initial_outcome = handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("initial indexing should succeed");
    assert!(
        initial_outcome.repair_reason.is_none(),
        "initial parse-backed indexing should not trigger repair"
    );

    let oversized = format!("fn gigantic() {{\n{}\n}}\n", "a".repeat(5_000_010));
    fs::write(&file_path, oversized).unwrap();

    let outcome = handle_file_created_or_modified_static(
        absolute_path,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("oversized update should be handled");

    assert!(
        outcome.repair_reason.is_none(),
        "oversized parser-backed files should downgrade to text-only, not extractor repair"
    );

    let db_lock = db.lock().unwrap();
    let symbols = db_lock.get_symbols_for_file("main.rs").unwrap();
    assert!(
        symbols.is_empty(),
        "oversized update should clear parser symbols and keep file as text-only"
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
    let temp_dir = julie_test_support::unique_temp_dir("incremental_indexing");
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
    let guard = acquire_gate("test_absolute_path_handling").await;

    println!("DEBUG: absolute_path = {}", absolute_path.display());
    println!("DEBUG: workspace_root = {}", workspace_root.display());

    // STEP 1: Index initial file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
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
        &guard,
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
        &guard,
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
        &guard,
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
        &guard,
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
    let temp_dir = julie_test_support::unique_temp_dir("file_deletion");
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
    let guard = acquire_gate("test_file_deletion").await;

    // Index the file
    handle_file_created_or_modified_static(
        absolute_path.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
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
    handle_file_deleted_static(absolute_path, &db, &workspace_root, None, &guard)
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
    let temp_dir = julie_test_support::unique_temp_dir("file_rename");
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
    let guard = acquire_gate("test_file_rename").await;

    // Index original file
    handle_file_created_or_modified_static(
        old_absolute.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
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
        &guard,
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
    let temp_dir = julie_test_support::unique_temp_dir("file_rename_destination_failure");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let old_file = workspace_root.join("old_name.rs");
    fs::write(&old_file, "fn old_function() {}").unwrap();
    let old_absolute = old_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_rename_destination_failure").await;

    handle_file_created_or_modified_static(
        old_absolute.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("Initial indexing should succeed");

    let new_file = workspace_root.join("new_name.rs");
    fs::write(&new_file, "fn destination_symbol() {}\n").unwrap();
    let initial_new_absolute = new_file.canonicalize().unwrap();

    handle_file_created_or_modified_static(
        initial_new_absolute,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
    )
    .await
    .expect("Initial destination indexing should succeed");

    fs::remove_file(&new_file).unwrap();
    fs::write(
        &old_file,
        "// Parser-backed destination content with no symbols should fail safely\n",
    )
    .unwrap();
    fs::rename(&old_file, &new_file).unwrap();
    let new_absolute = new_file.canonicalize().unwrap();

    handle_file_renamed_static(
        old_absolute,
        new_absolute,
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
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

    let new_symbols = db_lock.get_symbols_for_file("new_name.rs").unwrap();
    assert_eq!(
        new_symbols.len(),
        1,
        "failed destination should preserve its previous index"
    );
    assert_eq!(new_symbols[0].name, "destination_symbol");
}

#[tokio::test]
async fn test_file_rename_persists_repair_when_source_retirement_fails() {
    let temp_dir = julie_test_support::unique_temp_dir("file_rename_source_retirement_failure");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let old_file = workspace_root.join("old_name.rs");
    fs::write(&old_file, "fn old_function() {}\n").unwrap();
    let old_absolute = old_file.canonicalize().unwrap();

    let db_path = workspace_root.join("test.db");
    let db = Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ));
    let extractor_manager = Arc::new(ExtractorManager::new());
    let guard = acquire_gate("test_rename_source_retirement").await;

    handle_file_created_or_modified_static(
        old_absolute.clone(),
        &db,
        &extractor_manager,
        &workspace_root,
        None,
        &guard,
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
        &guard,
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
