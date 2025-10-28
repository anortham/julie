//! FTS5 Integrity Check and Auto-Rebuild Tests
//!
//! TDD: Write failing tests first, then implement integrity checking

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::database::SymbolDatabase;
use crate::extractors::Symbol;
use crate::extractors::base::Visibility;

/// Test helper: Create a simple test symbol
fn create_test_symbol(name: &str, file_path: &str) -> Symbol {
    use sha2::{Digest, Sha256};

    // Generate ID from name + file_path (like the extractors do)
    let id_input = format!("{}{}", name, file_path);
    let mut hasher = Sha256::new();
    hasher.update(id_input.as_bytes());
    let id = format!("{:x}", hasher.finalize());

    Symbol {
        id,
        name: name.to_string(),
        kind: crate::extractors::SymbolKind::Function,
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

#[test]
fn test_fts5_integrity_check_detects_missing_entries() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // Create database and populate with symbols
    {
        let mut db = SymbolDatabase::new(&db_path)?;

        let symbols = vec![
            create_test_symbol("function_one", "/test/file1.rs"),
            create_test_symbol("function_two", "/test/file2.rs"),
            create_test_symbol("function_three", "/test/file3.rs"),
        ];

        db.bulk_store_symbols(&symbols, "test_workspace")?;
    }

    // Reopen database and verify symbols exist
    {
        let db = SymbolDatabase::new(&db_path)?;
        let symbol_count = db.count_symbols_for_workspace()?;
        assert_eq!(symbol_count, 3, "Should have 3 symbols");
    }

    // SIMULATE CORRUPTION: Manually delete from FTS5 table but keep main symbols table
    {
        let db = SymbolDatabase::new(&db_path)?;

        // Disable ALL triggers to simulate corruption (bypassing automatic FTS5 sync)
        db.conn.execute("DROP TRIGGER IF EXISTS symbols_ai", [])?; // insert trigger
        db.conn.execute("DROP TRIGGER IF EXISTS symbols_ad", [])?; // delete trigger
        db.conn.execute("DROP TRIGGER IF EXISTS symbols_au", [])?; // update trigger

        // CRITICAL: FTS5 external content tables store index in shadow tables
        // Delete from the actual FTS5 shadow tables to corrupt the index
        db.conn.execute("DELETE FROM symbols_fts_data", [])?;
        db.conn.execute("DELETE FROM symbols_fts_idx", [])?;
        db.conn.execute("DELETE FROM symbols_fts_docsize", [])?;
        db.conn.execute("DELETE FROM symbols_fts_config", [])?;

        // Verify main table still has symbols
        let symbol_count = db.count_symbols_for_workspace()?;
        assert_eq!(symbol_count, 3, "Main table should still have 3 symbols");

        // Verify FTS5 index is corrupted (query should fail or return 0)
        let fts_is_corrupted = db.conn.query_row(
            "SELECT COUNT(*) FROM symbols_fts WHERE symbols_fts MATCH 'function'",
            [],
            |row| row.get::<_, i64>(0),
        ).is_err(); // Query fails due to "invalid fts5 file format"

        assert!(fts_is_corrupted, "FTS5 index should be corrupted (query should fail)");
    }

    // NOW TEST THE FIX: Reopen database and check if integrity check detects and fixes corruption
    {
        let db = SymbolDatabase::new(&db_path)?;

        // The integrity check should have run during new() and rebuilt FTS5
        // Verify FTS5 index can find symbols again (search works)
        let symbol_count = db.count_symbols_for_workspace()?;
        assert_eq!(symbol_count, 3, "Main table should still have 3 symbols");

        // Verify FTS5 search works (index was rebuilt)
        let fts_results = db.conn.query_row(
            "SELECT COUNT(*) FROM symbols_fts WHERE symbols_fts MATCH 'function'",
            [],
            |row| row.get::<_, i64>(0),
        )?;

        assert!(
            fts_results > 0,
            "FTS5 integrity check should have rebuilt index. Expected search results, got {}",
            fts_results
        );
    }

    Ok(())
}

#[test]
fn test_fts5_integrity_check_with_healthy_database() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // Create database and populate with symbols
    {
        let mut db = SymbolDatabase::new(&db_path)?;

        let symbols = vec![
            create_test_symbol("healthy_one", "/test/file1.rs"),
            create_test_symbol("healthy_two", "/test/file2.rs"),
        ];

        db.bulk_store_symbols(&symbols, "test_workspace")?;
    }

    // Reopen database - integrity check should pass without rebuilding
    {
        let db = SymbolDatabase::new(&db_path)?;

        // Verify both tables are in sync
        let symbol_count = db.count_symbols_for_workspace()?;
        let fts_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM symbols_fts",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(
            symbol_count, fts_count as usize,
            "Healthy database should have matching counts. symbols={}, FTS5={}",
            symbol_count, fts_count
        );
    }

    Ok(())
}

#[test]
fn test_files_fts5_integrity_check_detects_missing_entries() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");

    // Create database and store files
    {
        let db = SymbolDatabase::new(&db_path)?;

        // Store file info
        let test_file = temp_dir.path().join("test.rs");
        fs::write(&test_file, "fn test() {}")?;

        let file_info = crate::database::create_file_info(
            &test_file,
            "rust",
            temp_dir.path(),
        )?;

        db.store_file_info(&file_info)?;
    }

    // SIMULATE CORRUPTION: Delete from files_fts table
    {
        let db = SymbolDatabase::new(&db_path)?;

        // Disable ALL triggers to simulate corruption (bypassing automatic FTS5 sync)
        db.conn.execute("DROP TRIGGER IF EXISTS files_ai", [])?; // insert trigger
        db.conn.execute("DROP TRIGGER IF EXISTS files_ad", [])?; // delete trigger
        db.conn.execute("DROP TRIGGER IF EXISTS files_au", [])?; // update trigger

        // CRITICAL: FTS5 external content tables store index in shadow tables
        // Delete from the actual FTS5 shadow tables to corrupt the index
        db.conn.execute("DELETE FROM files_fts_data", [])?;
        db.conn.execute("DELETE FROM files_fts_idx", [])?;
        db.conn.execute("DELETE FROM files_fts_docsize", [])?;
        db.conn.execute("DELETE FROM files_fts_config", [])?;

        // Verify files table has entry
        let file_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM files",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(file_count, 1, "files table should have 1 entry");

        // Verify FTS5 index is corrupted (query should fail)
        let fts_is_corrupted = db.conn.query_row(
            "SELECT COUNT(*) FROM files_fts WHERE files_fts MATCH 'test'",
            [],
            |row| row.get::<_, i64>(0),
        ).is_err(); // Query fails due to "invalid fts5 file format"

        assert!(fts_is_corrupted, "FTS5 index should be corrupted (query should fail)");
    }

    // NOW TEST THE FIX: Reopen and verify integrity check rebuilds files_fts
    {
        let db = SymbolDatabase::new(&db_path)?;

        let file_count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM files",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(file_count, 1, "files table should still have 1 entry");

        // Verify FTS5 search works (index was rebuilt)
        let fts_results = db.conn.query_row(
            "SELECT COUNT(*) FROM files_fts WHERE files_fts MATCH 'test'",
            [],
            |row| row.get::<_, i64>(0),
        )?;

        assert!(
            fts_results > 0,
            "FTS5 integrity check should have rebuilt files_fts index. Expected search results, got {}",
            fts_results
        );
    }

    Ok(())
}
