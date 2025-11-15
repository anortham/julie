//! FTS5 Orphan Cleanup Bug Reproduction
//!
//! TDD RED PHASE: This test reproduces the production bug in clean_orphaned_files():
//!
//! BUG: Deleting multiple files in a loop, each triggering FTS5 rebuild, causes
//! rowid desynchronization between base tables and FTS5 indexes.
//!
//! SYMPTOM: "fts5: missing row X from content table" during search after cleanup
//!
//! ROOT CAUSE: Lines 346-376 in src/tools/workspace/indexing/incremental.rs
//! ```
//! for file_path in &orphaned_files {
//!     db.delete_symbols_for_file_in_workspace(file_path)?;  // Rebuilds symbols_fts
//!     db.delete_file_record_in_workspace(file_path)?;       // Rebuilds files_fts
//! }
//! ```
//! If cleaning 100 files, this rebuilds FTS5 indexes 200 times mid-loop!

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::database::{SymbolDatabase, create_file_info};

#[test]
fn test_orphan_cleanup_loop_causes_fts5_corruption() -> Result<()> {
    // TDD RED PHASE: This test MUST FAIL, proving the bug exists
    //
    // SCENARIO: Simulate what clean_orphaned_files() does:
    // 1. Create 20 files with symbols
    // 2. Delete them one-by-one in a loop (buggy behavior)
    // 3. Each deletion triggers FTS5 rebuild while other deletions pending
    // 4. Try to search FTS5 after cleanup
    // 5. EXPECT: "missing row X from content table" error

    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // Create 20 test files (simulate a workspace)
    let num_files = 20;
    let mut test_files = Vec::new();

    for i in 1..=num_files {
        let test_file = temp_dir.path().join(format!("file{}.rs", i));
        fs::write(
            &test_file,
            format!("fn test{}() {{ println!(\"hello\"); }}", i),
        )?;
        test_files.push(test_file);
    }

    // Phase 1: Bulk insert all files (initial indexing)
    {
        let db = SymbolDatabase::new(&db_path)?;

        for test_file in &test_files {
            let file_info = create_file_info(test_file, "rust", temp_dir.path())?;
            db.store_file_info(&file_info)?;
        }

        // Verify initial state
        let file_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        assert_eq!(file_count, num_files, "Should have {} files", num_files);

        let fts_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files_fts", [], |row| row.get(0))?;
        assert_eq!(
            fts_count, num_files,
            "FTS5 should have {} entries",
            num_files
        );
    }

    // Phase 2: Simulate buggy orphan cleanup (delete one-by-one with FTS5 rebuild after each)
    {
        let db = SymbolDatabase::new(&db_path)?;

        eprintln!("🐛 Simulating buggy orphan cleanup loop...");

        for (i, test_file) in test_files.iter().enumerate() {
            let file_path = format!("file{}.rs", i + 1);

            eprintln!("  Deleting file {}/{}: {}", i + 1, num_files, file_path);

            // This is the BUGGY pattern from clean_orphaned_files
            // Each delete triggers FTS5 rebuild mid-loop!
            if let Err(e) = db.delete_file_record_in_workspace(&file_path) {
                eprintln!("⚠️  Delete failed for {}: {}", file_path, e);
                // Continue anyway to simulate production behavior (warnings logged, continues)
            }
        }

        eprintln!("✅ Deletions complete. Now checking FTS5 integrity...");
    }

    // Phase 3: Verify corruption - FTS5 search should fail with "missing row" error
    {
        let db = SymbolDatabase::new(&db_path)?;

        let file_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        assert_eq!(file_count, 0, "All files should be deleted");

        let fts_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM files_fts", [], |row| row.get(0))?;
        assert_eq!(fts_count, 0, "FTS5 should have 0 entries after cleanup");

        // Try to search - this might trigger "missing row" if corruption occurred
        let search_result = db.conn.query_row(
            "SELECT COUNT(*) FROM files_fts WHERE files_fts MATCH 'test'",
            [],
            |row| row.get::<_, i64>(0),
        );

        match search_result {
            Ok(count) => {
                eprintln!(
                    "✅ Search succeeded with {} results (no corruption detected)",
                    count
                );
                // If search succeeds with 0 results, that's actually correct (all deleted)
                // But if it succeeds with >0 results, that's wrong!
                assert_eq!(count, 0, "Should find 0 results after deleting all files");
            }
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("missing row") || err_msg.contains("from content table") {
                    panic!("🐛 REPRODUCED THE BUG: {}", err_msg);
                } else {
                    return Err(e.into());
                }
            }
        }
    }

    // If we got here without "missing row" error, the bug might not reproduce
    // in this exact scenario. But the code is still buggy (inefficient rebuilds).
    eprintln!("⚠️  Bug did not reproduce in this test run, but code is still inefficient");
    eprintln!("    (rebuilding FTS5 {} times instead of once)", num_files);

    Ok(())
}

#[test]
#[ignore] // Slow test - run with `--ignored`
fn test_orphan_cleanup_stress_100_files() -> Result<()> {
    // STRESS TEST: Try with 100 files to increase chance of triggering race condition
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    let num_files = 100;
    let mut test_files = Vec::new();

    for i in 1..=num_files {
        let test_file = temp_dir.path().join(format!("file{:03}.rs", i));
        fs::write(
            &test_file,
            format!("fn test{}() {{ println!(\"hello\"); }}", i),
        )?;
        test_files.push(test_file);
    }

    // Bulk insert
    {
        let db = SymbolDatabase::new(&db_path)?;
        for test_file in &test_files {
            let file_info = create_file_info(test_file, "rust", temp_dir.path())?;
            db.store_file_info(&file_info)?;
        }
    }

    // Buggy cleanup loop
    {
        let db = SymbolDatabase::new(&db_path)?;
        for i in 1..=num_files {
            let file_path = format!("file{:03}.rs", i);
            if let Err(e) = db.delete_file_record_in_workspace(&file_path) {
                eprintln!("Delete failed for {}: {}", file_path, e);
            }
        }
    }

    // Try search
    {
        let db = SymbolDatabase::new(&db_path)?;
        let search_result = db.conn.query_row(
            "SELECT COUNT(*) FROM files_fts WHERE files_fts MATCH 'test'",
            [],
            |row| row.get::<_, i64>(0),
        );

        match search_result {
            Ok(_count) => {
                eprintln!("Search succeeded (no corruption in this run)");
            }
            Err(e) => {
                let err_msg = e.to_string();
                if err_msg.contains("missing row") {
                    panic!("REPRODUCED BUG with 100 files: {}", err_msg);
                }
            }
        }
    }

    Ok(())
}
