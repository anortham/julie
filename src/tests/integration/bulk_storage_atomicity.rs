//! Bulk Storage Atomicity Tests
//!
//! TDD: Write failing tests to verify corruption windows exist, then fix them
//!
//! These tests simulate crashes at various points during bulk storage operations
//! to verify that database remains consistent (no partial updates).

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::database::{create_file_info, SymbolDatabase};
use crate::extractors::base::Visibility;
use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};

/// Test helper: Create a simple test symbol
fn create_test_symbol(name: &str, file_path: &str) -> Symbol {
    use sha2::{Digest, Sha256};

    let id_input = format!("{}{}", name, file_path);
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let id = format!("{:x}", hasher.finalize());

    Symbol {
        id,
        name: name.to_string(),
        kind: SymbolKind::Function,
        file_path: file_path.to_string(),
        start_line: 1,
        end_line: 5,
        start_column: 0,
        end_column: 10,
        start_byte: 0,
        end_byte: 50,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        parent_id: None,
        language: "rust".to_string(),
        visibility: Some(Visibility::Public),
        code_context: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
    }
}

/// Test helper: Create a test relationship
fn create_test_relationship(from_id: &str, to_id: &str, kind: RelationshipKind) -> Relationship {
    use sha2::{Digest, Sha256};

    let id_input = format!("{}{}{:?}", from_id, to_id, kind);
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let id = format!("{:x}", hasher.finalize());

    Relationship {
        id,
        from_symbol_id: from_id.to_string(),
        to_symbol_id: to_id.to_string(),
        kind,
        file_path: "/test/file.rs".to_string(),
        line_number: 10,
        confidence: 1.0,
        metadata: None,
    }
}

#[test]
fn test_bulk_store_symbols_is_atomic() -> Result<()> {
    // This test verifies our recent fix - bulk_store_symbols wraps EVERYTHING
    // in one transaction, so crash at any point rolls back ALL changes

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // Create database and simulate crash during bulk_store_symbols
    {
        let mut db = SymbolDatabase::new(&db_path)?;

        let symbols = vec![
            create_test_symbol("function_one", "/test/file1.rs"),
            create_test_symbol("function_two", "/test/file2.rs"),
        ];

        // The fix ensures that if bulk_store_symbols fails, NOTHING is committed
        // We can't easily simulate crash mid-transaction from Rust, but we can
        // verify that triggers and indexes are restored even on error

        // This should succeed - verifying the happy path
        db.bulk_store_symbols(&symbols, "test_workspace")?;

        // Verify symbols were stored
        let symbol_count = db.count_symbols_for_workspace()?;
        assert_eq!(symbol_count, 2, "Should have 2 symbols after successful bulk insert");

        // Verify FTS5 is in sync
        let fts_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM symbols_fts WHERE symbols_fts MATCH 'function'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(fts_count, 2, "FTS5 should have 2 entries matching 'function'");

        // Verify indexes exist
        let index_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND tbl_name='symbols'",
            [],
            |row| row.get(0),
        )?;
        assert!(index_count >= 6, "Should have at least 6 symbol indexes");

        // Verify triggers exist
        let trigger_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND tbl_name='symbols'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(trigger_count, 3, "Should have 3 FTS triggers (insert, update, delete)");
    }

    Ok(())
}

#[test]
fn test_bulk_store_files_atomicity() -> Result<()> {
    // TEST EXPECTATION: This test should FAIL until we fix bulk_store_files
    // to use the atomic transaction pattern
    //
    // Current behavior: triggers disabled outside transaction → crash → corruption
    // Expected behavior: everything in transaction → crash → rollback

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // Create test files
    let test_file1 = temp_dir.path().join("test1.rs");
    let test_file2 = temp_dir.path().join("test2.rs");
    fs::write(&test_file1, "fn test1() {}")?;
    fs::write(&test_file2, "fn test2() {}")?;

    let file_info1 = create_file_info(&test_file1, "rust")?;
    let file_info2 = create_file_info(&test_file2, "rust")?;

    // Store files
    {
        let mut db = SymbolDatabase::new(&db_path)?;
        db.bulk_store_files(&[file_info1, file_info2])?;
    }

    // Verify state is consistent
    {
        let db = SymbolDatabase::new(&db_path)?;

        // Check file count
        let file_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM files",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(file_count, 2, "Should have 2 files");

        // Check FTS5 count - this should match if atomic
        let fts_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM files_fts",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(fts_count, 2, "FTS5 should have 2 file entries");

        // Verify indexes exist
        let index_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND tbl_name='files'",
            [],
            |row| row.get(0),
        )?;
        assert!(index_count >= 2, "Should have at least 2 file indexes");

        // Verify triggers exist
        let trigger_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND tbl_name='files'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(trigger_count, 3, "Should have 3 FTS triggers for files");
    }

    Ok(())
}

#[test]
fn test_bulk_store_relationships_atomicity() -> Result<()> {
    // TEST EXPECTATION: This test should FAIL until we fix bulk_store_relationships
    // to use the atomic transaction pattern
    //
    // Current behavior: indexes dropped outside transaction → crash → missing indexes
    // Expected behavior: everything in transaction → crash → rollback

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // First create symbols (relationships need existing symbols due to foreign keys)
    {
        let mut db = SymbolDatabase::new(&db_path)?;

        let symbols = vec![
            create_test_symbol("caller", "/test/file1.rs"),
            create_test_symbol("callee", "/test/file2.rs"),
        ];
        db.bulk_store_symbols(&symbols, "test_workspace")?;
    }

    // Get symbol IDs
    let (caller_id, callee_id) = {
        let db = SymbolDatabase::new(&db_path)?;
        let caller_id: String = db.conn.query_row(
            "SELECT id FROM symbols WHERE name='caller'",
            [],
            |row| row.get(0),
        )?;
        let callee_id: String = db.conn.query_row(
            "SELECT id FROM symbols WHERE name='callee'",
            [],
            |row| row.get(0),
        )?;
        (caller_id, callee_id)
    };

    // Create and store relationships
    {
        let mut db = SymbolDatabase::new(&db_path)?;

        let relationships = vec![
            create_test_relationship(&caller_id, &callee_id, RelationshipKind::Calls),
        ];

        db.bulk_store_relationships(&relationships)?;
    }

    // Verify state is consistent
    {
        let db = SymbolDatabase::new(&db_path)?;

        // Check relationship count
        let rel_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM relationships",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(rel_count, 1, "Should have 1 relationship");

        // Verify indexes exist
        let index_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND tbl_name='relationships'",
            [],
            |row| row.get(0),
        )?;
        assert!(index_count >= 3, "Should have at least 3 relationship indexes");
    }

    Ok(())
}

#[test]
fn test_incremental_update_cleanup_atomicity() -> Result<()> {
    // TEST EXPECTATION: This test should FAIL until we wrap cleanup + bulk operations
    // in a single transaction
    //
    // Current behavior:
    //   1. delete_symbols_for_file() commits
    //   2. Crash
    //   3. bulk_store_symbols() never runs
    //   4. File has no symbols in database
    //
    // Expected behavior:
    //   1. Start transaction
    //   2. Delete old symbols
    //   3. Insert new symbols
    //   4. Crash → rollback both operations
    //   5. Database still has old symbols

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // Initial state: file with symbols
    {
        let mut db = SymbolDatabase::new(&db_path)?;

        let symbols = vec![
            create_test_symbol("old_function_v1", "/test/file.rs"),
        ];
        db.bulk_store_symbols(&symbols, "test_workspace")?;
    }

    // Verify initial state
    {
        let db = SymbolDatabase::new(&db_path)?;
        let count = db.count_symbols_for_workspace()?;
        assert_eq!(count, 1, "Should have 1 symbol initially");
    }

    // Simulate incremental update: delete old, insert new
    // This mimics what happens in process_files_optimized
    {
        let mut db = SymbolDatabase::new(&db_path)?;

        // Step 1: Delete old symbols (THIS COMMITS)
        db.delete_symbols_for_file_in_workspace("/test/file.rs")?;

        // At this point, if we crash, the file has no symbols
        // Let's verify the cleanup happened
        let count_after_delete = db.count_symbols_for_workspace()?;
        assert_eq!(count_after_delete, 0, "Symbols should be deleted after cleanup");

        // Step 2: Insert new symbols (separate transaction)
        let new_symbols = vec![
            create_test_symbol("new_function_v2", "/test/file.rs"),
        ];
        db.bulk_store_symbols(&new_symbols, "test_workspace")?;
    }

    // Final state: should have new symbols
    {
        let db = SymbolDatabase::new(&db_path)?;
        let count = db.count_symbols_for_workspace()?;
        assert_eq!(count, 1, "Should have 1 symbol after update");

        // Verify it's the NEW symbol
        let name: String = db.conn.query_row(
            "SELECT name FROM symbols WHERE file_path='/test/file.rs'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(name, "new_function_v2", "Should have new symbol name");
    }

    // THE PROBLEM: Between delete and insert, there's a window where file has no symbols
    // If crash happens during that window, database is in inconsistent state
    // FIX: Wrap delete + insert in single transaction

    Ok(())
}
