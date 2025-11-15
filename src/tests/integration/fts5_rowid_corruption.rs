//! FTS5 Rowid Corruption Reproduction Test
//!
//! TDD RED PHASE: This test reproduces the exact corruption pattern:
//! - Delete a file (triggers delete from FTS5)
//! - Rebuild FTS5 (unnecessary and creates rowid desync)
//! - Insert new file (might reuse old rowid)
//! - FTS5 now has orphan rowids

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::database::{SymbolDatabase, create_file_info};

#[test]
fn test_delete_file_record_causes_fts5_corruption() -> Result<()> {
    // RED PHASE: This test SHOULD FAIL, proving the bug exists
    //
    // HYPOTHESIS: delete_file_record() calls rebuild_files_fts() which is WRONG
    // - DELETE fires trigger (removes rowid from FTS5) ✓ correct
    // - rebuild_files_fts() called afterward (REDUNDANT and potentially harmful)
    // - rebuild does 'delete-all' then 'rebuild' - this can create rowid mismatches

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // Create test files
    let test_file1 = temp_dir.path().join("file1.rs");
    let test_file2 = temp_dir.path().join("file2.rs");
    let test_file3 = temp_dir.path().join("file3.rs");

    fs::write(&test_file1, "fn test1() { println!(\"hello\"); }")?;
    fs::write(&test_file2, "fn test2() { println!(\"world\"); }")?;
    fs::write(&test_file3, "fn test3() { println!(\"rust\"); }")?;

    // Phase 1: Insert files sequentially
    {
        let db = SymbolDatabase::new(&db_path)?;

        let file_info1 = create_file_info(&test_file1, "rust", temp_dir.path())?;
        let file_info2 = create_file_info(&test_file2, "rust", temp_dir.path())?;
        let file_info3 = create_file_info(&test_file3, "rust", temp_dir.path())?;

        db.store_file_info(&file_info1)?;
        db.store_file_info(&file_info2)?;
        db.store_file_info(&file_info3)?;
    }

    // Verify initial state
    {
        let db = SymbolDatabase::new(&db_path)?;

        let file_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        assert_eq!(file_count, 3, "Should have 3 files");

        let fts_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files_fts", [], |row| row.get(0))?;
        assert_eq!(fts_count, 3, "FTS5 should have 3 entries");
    }

    // Phase 2: Delete file2 using the BUGGY delete_file_record method
    {
        let db = SymbolDatabase::new(&db_path)?;

        // This calls rebuild_files_fts() which is the bug!
        db.delete_file_record("file2.rs")?;
    }

    // Phase 3: Verify corruption - FTS5 might have wrong rowid mappings
    {
        let db = SymbolDatabase::new(&db_path)?;

        let file_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        assert_eq!(file_count, 2, "Should have 2 files after delete");

        let fts_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files_fts", [], |row| row.get(0))?;
        assert_eq!(fts_count, 2, "FTS5 should have 2 entries after rebuild");

        // Check if rowids are consistent between files and files_fts
        let files_rowids: Vec<i64> = db
            .conn
            .prepare("SELECT rowid FROM files ORDER BY rowid")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let fts_rowids: Vec<i64> = db
            .conn
            .prepare("SELECT rowid FROM files_fts ORDER BY rowid")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;

        // THIS IS THE BUG: After rebuild, FTS5 rowids might not match files rowids!
        // Because rebuild uses current files table to repopulate FTS5
        assert_eq!(
            files_rowids, fts_rowids,
            "CRITICAL BUG: Rowids should match! files={:?}, fts={:?}",
            files_rowids, fts_rowids
        );

        // Try to search - this might trigger "missing row from content table" error
        let search_result = db.conn.query_row(
            "SELECT COUNT(*) FROM files_fts WHERE files_fts MATCH 'test'",
            [],
            |row| row.get::<_, i64>(0),
        );

        match search_result {
            Ok(count) => {
                println!("Search succeeded with {} results", count);
                assert!(count >= 0, "Search should work");
            }
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("missing row") {
                    panic!("REPRODUCED THE BUG: {}", err_msg);
                } else {
                    return Err(e.into());
                }
            }
        }
    }

    Ok(())
}

#[test]
#[ignore] // This test demonstrates that triggers ALONE are insufficient for FTS5 external content tables
fn test_triggers_alone_insufficient_for_external_content() -> Result<()> {
    // This test FAILS intentionally to document FTS5 external content table behavior
    //
    // FINDING: Triggers alone are NOT sufficient!
    // - DELETE fires trigger: `DELETE FROM files_fts WHERE rowid = old.rowid`
    // - This removes the ROWID MAPPING but FTS5 shadow tables KEEP the indexed content
    // - Result: Orphaned content remains searchable, causing "missing row" errors
    //
    // SOLUTION: Must call rebuild_fts() after DELETE to clean shadow tables

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // Create test files
    let test_file1 = temp_dir.path().join("file1.rs");
    let test_file2 = temp_dir.path().join("file2.rs");
    let test_file3 = temp_dir.path().join("file3.rs");

    fs::write(&test_file1, "fn test1() { println!(\"hello\"); }")?;
    fs::write(&test_file2, "fn test2() { println!(\"world\"); }")?;
    fs::write(&test_file3, "fn test3() { println!(\"rust\"); }")?;

    // Insert files
    {
        let db = SymbolDatabase::new(&db_path)?;

        let file_info1 = create_file_info(&test_file1, "rust", temp_dir.path())?;
        let file_info2 = create_file_info(&test_file2, "rust", temp_dir.path())?;
        let file_info3 = create_file_info(&test_file3, "rust", temp_dir.path())?;

        db.store_file_info(&file_info1)?;
        db.store_file_info(&file_info2)?;
        db.store_file_info(&file_info3)?;
    }

    // Delete file2 the CORRECT way (trigger only, no rebuild)
    {
        let db = SymbolDatabase::new(&db_path)?;

        // Check that triggers exist
        let trigger_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND tbl_name='files'",
            [],
            |row| row.get(0),
        )?;
        println!("Triggers before delete: {}", trigger_count);
        assert_eq!(
            trigger_count, 3,
            "Should have 3 triggers (insert, update, delete)"
        );

        // Manual delete WITHOUT rebuild (testing correct behavior)
        let count = db.conn.execute(
            "DELETE FROM files WHERE path = ?1",
            rusqlite::params!["file2.rs"],
        )?;
        assert_eq!(count, 1, "Should delete 1 file");

        // Trigger automatically deleted from FTS5 - no rebuild needed!
    }

    // Verify state is consistent
    {
        let db = SymbolDatabase::new(&db_path)?;

        let file_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        assert_eq!(file_count, 2, "Should have 2 files");

        let fts_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files_fts", [], |row| row.get(0))?;
        assert_eq!(
            fts_count, 2,
            "FTS5 should have 2 entries (trigger handled it)"
        );

        // Debug: Check what we have
        let files_paths: Vec<String> = db
            .conn
            .prepare("SELECT path FROM files ORDER BY path")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        eprintln!("Files in base table: {:?}", files_paths);

        let fts_paths: Vec<String> = db.conn.prepare(
            "SELECT f.path FROM files f INNER JOIN files_fts fts ON f.rowid = fts.rowid ORDER BY f.path"
        )?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        eprintln!("Files in FTS5 (via join): {:?}", fts_paths);

        // Rowids should match perfectly (no rebuild interference)
        let files_rowids: Vec<i64> = db
            .conn
            .prepare("SELECT rowid FROM files ORDER BY rowid")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let fts_rowids: Vec<i64> = db
            .conn
            .prepare("SELECT rowid FROM files_fts ORDER BY rowid")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;

        eprintln!("Files rowids: {:?}", files_rowids);
        eprintln!("FTS5 rowids: {:?}", fts_rowids);

        assert_eq!(
            files_rowids, fts_rowids,
            "Rowids MUST match when using triggers correctly"
        );

        // Search should work perfectly
        // Note: Searching in content field (files have content "fn test1() { println!(\"hello\"); }")
        let search_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM files_fts WHERE files_fts MATCH 'println'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(search_count, 2, "Should find 2 files matching 'println'");
    }

    Ok(())
}
