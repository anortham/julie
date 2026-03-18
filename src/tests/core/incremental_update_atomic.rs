// Tests for incremental_update_atomic — the core write path for incremental indexing.
//
// This method wraps cleanup + bulk insert in a single transaction to prevent
// corruption if a crash occurs between delete and insert phases.

use crate::database::types::FileInfo;
use crate::database::SymbolDatabase;
use crate::extractors::base::TypeInfo;
use crate::extractors::{Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind};
use tempfile::TempDir;

/// Helper: build a minimal Symbol with the given id, name, and file_path.
fn make_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 10,
        end_column: 0,
        start_byte: 0,
        end_byte: 100,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    }
}

/// Helper: build a minimal FileInfo.
fn make_file(path: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash_{}", path),
        size: 200,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 0,
        content: None,
    }
}

/// Helper: build a minimal Relationship.
fn make_relationship(id: &str, from: &str, to: &str, file_path: &str) -> Relationship {
    Relationship {
        id: id.to_string(),
        from_symbol_id: from.to_string(),
        to_symbol_id: to.to_string(),
        kind: RelationshipKind::Calls,
        file_path: file_path.to_string(),
        line_number: 5,
        confidence: 1.0,
        metadata: None,
    }
}

/// Helper: build a minimal Identifier.
fn make_identifier(id: &str, name: &str, file_path: &str) -> Identifier {
    Identifier {
        id: id.to_string(),
        name: name.to_string(),
        kind: IdentifierKind::Call,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 3,
        start_column: 4,
        end_line: 3,
        end_column: 20,
        start_byte: 30,
        end_byte: 46,
        containing_symbol_id: None,
        target_symbol_id: None,
        confidence: 1.0,
        code_context: None,
    }
}

/// Helper: build a minimal TypeInfo.
fn make_type_info(symbol_id: &str, resolved_type: &str) -> TypeInfo {
    TypeInfo {
        symbol_id: symbol_id.to_string(),
        resolved_type: resolved_type.to_string(),
        generic_params: None,
        constraints: None,
        is_inferred: false,
        language: "rust".to_string(),
        metadata: None,
    }
}

/// Helper: count rows in a table.
fn count_rows(db: &SymbolDatabase, table: &str) -> i64 {
    db.conn
        .query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| {
            row.get(0)
        })
        .unwrap()
}

// ---------------------------------------------------------------------------
// Test 1: Basic insert — files + symbols + relationships + identifiers + types
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_basic_insert() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file("src/main.rs")];
    let symbols = vec![
        make_symbol("sym_a", "do_stuff", "src/main.rs"),
        make_symbol("sym_b", "helper", "src/main.rs"),
    ];
    let relationships = vec![make_relationship("rel_1", "sym_a", "sym_b", "src/main.rs")];
    let identifiers = vec![make_identifier("ident_1", "helper", "src/main.rs")];
    let types = vec![make_type_info("sym_a", "Result<(), Error>")];

    db.incremental_update_atomic(
        &[],           // nothing to clean
        &files,
        &symbols,
        &relationships,
        &identifiers,
        &types,
        "ws_test",
    )
    .expect("incremental_update_atomic should succeed");

    // Verify file was inserted
    assert_eq!(count_rows(&db, "files"), 1);

    // Verify symbols
    assert_eq!(count_rows(&db, "symbols"), 2);
    let all_symbols = db.get_all_symbols().unwrap();
    let names: Vec<&str> = all_symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"do_stuff"), "should contain do_stuff");
    assert!(names.contains(&"helper"), "should contain helper");

    // Verify relationship
    assert_eq!(count_rows(&db, "relationships"), 1);

    // Verify identifier
    assert_eq!(count_rows(&db, "identifiers"), 1);

    // Verify type
    assert_eq!(count_rows(&db, "types"), 1);
    let resolved: String = db
        .conn
        .query_row(
            "SELECT resolved_type FROM types WHERE symbol_id = ?1",
            ["sym_a"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(resolved, "Result<(), Error>");
}

// ---------------------------------------------------------------------------
// Test 2: Incremental update — clean old data, insert replacement
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_clean_and_replace() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // --- Round 1: initial indexing of file_a with 2 symbols ---
    let files_v1 = vec![make_file("file_a.rs")];
    let symbols_v1 = vec![
        make_symbol("old_1", "alpha", "file_a.rs"),
        make_symbol("old_2", "beta", "file_a.rs"),
    ];
    let rels_v1 = vec![make_relationship("rel_old", "old_1", "old_2", "file_a.rs")];
    let idents_v1 = vec![make_identifier("id_old", "beta", "file_a.rs")];
    let types_v1 = vec![make_type_info("old_1", "i32")];

    db.incremental_update_atomic(
        &[],
        &files_v1,
        &symbols_v1,
        &rels_v1,
        &idents_v1,
        &types_v1,
        "ws_test",
    )
    .unwrap();

    assert_eq!(count_rows(&db, "symbols"), 2);
    assert_eq!(count_rows(&db, "relationships"), 1);
    assert_eq!(count_rows(&db, "identifiers"), 1);
    assert_eq!(count_rows(&db, "types"), 1);

    // --- Round 2: file_a was modified — clean old, insert 3 new symbols ---
    let files_v2 = vec![make_file("file_a.rs")];
    let symbols_v2 = vec![
        make_symbol("new_1", "gamma", "file_a.rs"),
        make_symbol("new_2", "delta", "file_a.rs"),
        make_symbol("new_3", "epsilon", "file_a.rs"),
    ];
    let rels_v2 = vec![
        make_relationship("rel_new_1", "new_1", "new_2", "file_a.rs"),
        make_relationship("rel_new_2", "new_2", "new_3", "file_a.rs"),
    ];
    let idents_v2 = vec![
        make_identifier("id_new_1", "delta", "file_a.rs"),
        make_identifier("id_new_2", "epsilon", "file_a.rs"),
    ];
    let types_v2 = vec![
        make_type_info("new_1", "String"),
        make_type_info("new_2", "Vec<u8>"),
    ];

    db.incremental_update_atomic(
        &["file_a.rs".to_string()], // clean the old file
        &files_v2,
        &symbols_v2,
        &rels_v2,
        &idents_v2,
        &types_v2,
        "ws_test",
    )
    .unwrap();

    // Old symbols should be gone, new ones present
    assert_eq!(count_rows(&db, "symbols"), 3, "should have 3 new symbols");
    let all_symbols = db.get_all_symbols().unwrap();
    let names: Vec<&str> = all_symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(!names.contains(&"alpha"), "old symbol alpha should be gone");
    assert!(!names.contains(&"beta"), "old symbol beta should be gone");
    assert!(names.contains(&"gamma"), "new symbol gamma should exist");
    assert!(names.contains(&"delta"), "new symbol delta should exist");
    assert!(names.contains(&"epsilon"), "new symbol epsilon should exist");

    // Old relationships gone, new ones present
    assert_eq!(count_rows(&db, "relationships"), 2, "should have 2 new relationships");

    // Old identifiers gone, new ones present
    assert_eq!(count_rows(&db, "identifiers"), 2, "should have 2 new identifiers");

    // Old types gone, new ones present
    assert_eq!(count_rows(&db, "types"), 2, "should have 2 new types");
}

// ---------------------------------------------------------------------------
// Test 3: Multi-file — cleaning one file doesn't affect another
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_multi_file_isolation() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert data for two files in one call
    let files = vec![make_file("lib.rs"), make_file("main.rs")];
    let symbols = vec![
        make_symbol("lib_fn", "lib_function", "lib.rs"),
        make_symbol("main_fn", "main_function", "main.rs"),
    ];
    let idents = vec![
        make_identifier("id_lib", "something", "lib.rs"),
        make_identifier("id_main", "something_else", "main.rs"),
    ];

    db.incremental_update_atomic(&[], &files, &symbols, &[], &idents, &[], "ws_test")
        .unwrap();

    assert_eq!(count_rows(&db, "symbols"), 2);
    assert_eq!(count_rows(&db, "identifiers"), 2);

    // Now re-index only lib.rs — main.rs data should be untouched
    let new_files = vec![make_file("lib.rs")];
    let new_symbols = vec![make_symbol("lib_fn_v2", "lib_function_v2", "lib.rs")];
    let new_idents = vec![make_identifier("id_lib_v2", "new_ref", "lib.rs")];

    db.incremental_update_atomic(
        &["lib.rs".to_string()],
        &new_files,
        &new_symbols,
        &[],
        &new_idents,
        &[],
        "ws_test",
    )
    .unwrap();

    // main.rs symbol still present
    let all_symbols = db.get_all_symbols().unwrap();
    let names: Vec<&str> = all_symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"main_function"), "main.rs symbol untouched");
    assert!(
        names.contains(&"lib_function_v2"),
        "lib.rs updated symbol present"
    );
    assert!(
        !names.contains(&"lib_function"),
        "old lib.rs symbol should be gone"
    );
    assert_eq!(all_symbols.len(), 2, "total symbol count should still be 2");

    // Identifiers: main.rs identifier untouched, lib.rs replaced
    assert_eq!(count_rows(&db, "identifiers"), 2, "total identifiers should be 2");
    let lib_ident_name: String = db
        .conn
        .query_row(
            "SELECT name FROM identifiers WHERE file_path = 'lib.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(lib_ident_name, "new_ref", "lib.rs identifier should be updated");
}

// ---------------------------------------------------------------------------
// Test 4: Empty inputs — no crash, no-op
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_empty_inputs() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Calling with all-empty inputs should succeed (no-op)
    db.incremental_update_atomic(&[], &[], &[], &[], &[], &[], "ws_test")
        .expect("empty incremental_update_atomic should succeed");

    assert_eq!(count_rows(&db, "files"), 0);
    assert_eq!(count_rows(&db, "symbols"), 0);
}

// ---------------------------------------------------------------------------
// Test 5: Cleaning a file that doesn't exist is a no-op (no error)
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_clean_nonexistent_file() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Clean a file that was never indexed — should succeed silently
    db.incremental_update_atomic(
        &["ghost.rs".to_string()],
        &[],
        &[],
        &[],
        &[],
        &[],
        "ws_test",
    )
    .expect("cleaning non-existent file should not error");

    assert_eq!(count_rows(&db, "symbols"), 0);
}
