// Tests for incremental_update_atomic — the core write path for incremental indexing.
//
// This method wraps cleanup + bulk insert in a single transaction to prevent
// corruption if a crash occurs between delete and insert phases.

use crate::database::SymbolDatabase;
use crate::database::bulk::atomic::{AtomicPersistenceMetadata, CanonicalWriteSet};
use crate::database::bulk::type_arguments::{TypeArgumentRow, flatten_type_argument_usages};
use crate::database::types::FileInfo;
use crate::extractors::base::{TypeArgument, TypeArgumentUsage, TypeInfo};
use crate::extractors::{
    Identifier, IdentifierKind, Literal, LiteralKind, Relationship, RelationshipKind, Symbol,
    SymbolKind,
};
use tempfile::TempDir;

/// Helper: build a minimal carrier-gated Literal row (post-classification
/// state — a recognized `kind` and a `carrier`, as the write path stores it).
fn make_literal(
    id: &str,
    text: &str,
    kind: LiteralKind,
    carrier: &str,
    file_path: &str,
) -> Literal {
    Literal {
        id: id.to_string(),
        literal_text: text.to_string(),
        kind,
        carrier: Some(carrier.to_string()),
        arg_position: 0,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 4,
        start_column: 8,
        end_line: 4,
        end_column: 24,
        start_byte: 40,
        end_byte: 56,
        containing_symbol_id: None,
        confidence: 1.0,
    }
}

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
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
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
        line_count: 0,
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

/// Helper: build a minimal Identifier with explicit symbol references.
fn make_identifier_with_refs(
    id: &str,
    name: &str,
    file_path: &str,
    containing_symbol_id: Option<&str>,
    target_symbol_id: Option<&str>,
) -> Identifier {
    let mut identifier = make_identifier(id, name, file_path);
    identifier.containing_symbol_id = containing_symbol_id.map(str::to_string);
    identifier.target_symbol_id = target_symbol_id.map(str::to_string);
    identifier
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

/// Helper: count rows in a table matching a WHERE clause.
fn count_rows_where(db: &SymbolDatabase, table: &str, where_clause: &str) -> i64 {
    db.conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {} WHERE {}", table, where_clause),
            [],
            |row| row.get(0),
        )
        .unwrap()
}

/// Helper: a leaf type argument with no nested generics.
fn leaf_arg(ordinal: u32, type_name: &str) -> TypeArgument {
    TypeArgument {
        ordinal,
        type_name: type_name.to_string(),
        children: Vec::new(),
    }
}

/// Helper: flatten one use-site's type-argument tree into rows using the
/// production flatten, so row ids and parent links match the real write path.
fn type_argument_rows(
    identifier_id: &str,
    file_path: &str,
    arguments: Vec<TypeArgument>,
) -> Vec<TypeArgumentRow> {
    flatten_type_argument_usages(&[TypeArgumentUsage {
        identifier_id: identifier_id.to_string(),
        file_path: file_path.to_string(),
        language: "csharp".to_string(),
        arguments,
    }])
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
        &[], // nothing to clean
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
    assert!(
        names.contains(&"epsilon"),
        "new symbol epsilon should exist"
    );

    // Old relationships gone, new ones present
    assert_eq!(
        count_rows(&db, "relationships"),
        2,
        "should have 2 new relationships"
    );

    // Old identifiers gone, new ones present
    assert_eq!(
        count_rows(&db, "identifiers"),
        2,
        "should have 2 new identifiers"
    );

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
    assert_eq!(
        count_rows(&db, "identifiers"),
        2,
        "total identifiers should be 2"
    );
    let lib_ident_name: String = db
        .conn
        .query_row(
            "SELECT name FROM identifiers WHERE file_path = 'lib.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        lib_ident_name, "new_ref",
        "lib.rs identifier should be updated"
    );
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
    assert_eq!(
        db.get_latest_canonical_revision("ws_test")
            .expect("revision lookup should succeed"),
        None,
        "no-op incremental writes must not advance canonical revision"
    );
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

// ---------------------------------------------------------------------------
// Test 6: Invalid relationships should be skipped when symbols are missing
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_skips_relationships_with_missing_symbols() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file("src/main.rs")];
    let symbols = vec![
        make_symbol("sym_a", "do_stuff", "src/main.rs"),
        make_symbol("sym_b", "helper", "src/main.rs"),
    ];

    let valid = make_relationship("rel_valid", "sym_a", "sym_b", "src/main.rs");
    let invalid = make_relationship("rel_invalid", "sym_a", "missing_symbol", "src/main.rs");

    db.incremental_update_atomic(
        &[],
        &files,
        &symbols,
        &[valid, invalid],
        &[],
        &[],
        "ws_test",
    )
    .expect("incremental_update_atomic should succeed");

    let relationship_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        relationship_count, 1,
        "invalid relationship should be skipped"
    );

    let dangling_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM relationships WHERE to_symbol_id = 'missing_symbol'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(dangling_count, 0, "must not persist dangling relationship");

    let revision = db
        .get_latest_canonical_revision("ws_test")
        .expect("revision lookup should succeed")
        .expect("revision should be recorded for non-empty writes");
    assert_eq!(
        revision.relationship_count, 1,
        "revision counts must reflect persisted relationships, not skipped ones"
    );
}

// ---------------------------------------------------------------------------
// Test 7: Invalid identifier refs should be normalized to NULL
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_nulls_invalid_identifier_refs() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file("src/main.rs")];
    let symbols = vec![make_symbol("sym_a", "do_stuff", "src/main.rs")];
    let identifiers = vec![make_identifier_with_refs(
        "ident_1",
        "helper",
        "src/main.rs",
        Some("missing_containing"),
        Some("missing_target"),
    )];

    db.incremental_update_atomic(&[], &files, &symbols, &[], &identifiers, &[], "ws_test")
        .expect("incremental_update_atomic should succeed");

    let (containing, target): (Option<String>, Option<String>) = db
        .conn
        .query_row(
            "SELECT containing_symbol_id, target_symbol_id FROM identifiers WHERE id = ?1",
            ["ident_1"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(
        containing, None,
        "invalid containing_symbol_id must be normalized to NULL"
    );
    assert_eq!(
        target, None,
        "invalid target_symbol_id must be normalized to NULL"
    );
}

// ---------------------------------------------------------------------------
// Test 8: Type rows must reference existing symbols
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_skips_types_with_missing_symbols() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file("src/main.rs")];
    let symbols = vec![make_symbol("sym_a", "do_stuff", "src/main.rs")];
    let types = vec![
        make_type_info("sym_a", "Result<(), Error>"),
        make_type_info("missing_symbol", "String"),
    ];

    db.incremental_update_atomic(&[], &files, &symbols, &[], &[], &types, "ws_test")
        .expect("incremental_update_atomic should succeed");

    let type_count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM types", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        type_count, 1,
        "type rows with missing symbols must be skipped"
    );

    let missing_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM types WHERE symbol_id = 'missing_symbol'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(missing_count, 0, "must not persist dangling type rows");

    let revision = db
        .get_latest_canonical_revision("ws_test")
        .expect("revision lookup should succeed")
        .expect("revision should be recorded for non-empty writes");
    assert_eq!(
        revision.type_count, 1,
        "revision counts must reflect persisted types, not skipped ones"
    );
}

// ---------------------------------------------------------------------------
// Test 12: Empty fresh writes should not record a canonical revision
// ---------------------------------------------------------------------------
#[test]
fn test_bulk_store_fresh_atomic_empty_inputs_do_not_record_canonical_revision() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.bulk_store_fresh_atomic(&[], &[], &[], &[], &[], "ws_test")
        .expect("empty bulk_store_fresh_atomic should succeed");

    assert_eq!(
        db.get_latest_canonical_revision("ws_test")
            .expect("revision lookup should succeed"),
        None,
        "no-op fresh writes must not advance canonical revision"
    );
}

// ---------------------------------------------------------------------------
// Test 9: Canonical revision should advance on incremental writes
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_records_canonical_revision() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.incremental_update_atomic(
        &[],
        &[make_file("src/main.rs")],
        &[make_symbol("sym_a", "do_stuff", "src/main.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )
    .expect("incremental_update_atomic should succeed");

    let revision = db
        .get_latest_canonical_revision("ws_test")
        .expect("revision lookup should succeed")
        .expect("incremental write should record a revision");

    assert_eq!(revision.workspace_id, "ws_test");
    assert_eq!(revision.revision, 1);
    assert_eq!(revision.kind.as_str(), "incremental");
    assert_eq!(revision.file_count, 1);
    assert_eq!(revision.symbol_count, 1);

    let usage = db
        .get_workspace_usage_stats("ws_test")
        .expect("workspace usage stats should succeed");
    assert_eq!(usage.canonical_revision, Some(1));
}

// ---------------------------------------------------------------------------
// Test 10: Canonical revision should advance on fresh writes
// ---------------------------------------------------------------------------
#[test]
fn test_bulk_store_fresh_atomic_records_canonical_revision() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.bulk_store_fresh_atomic(
        &[make_file("src/lib.rs")],
        &[make_symbol("sym_a", "fresh_symbol", "src/lib.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )
    .expect("bulk_store_fresh_atomic should succeed");

    let revision = db
        .get_latest_canonical_revision("ws_test")
        .expect("revision lookup should succeed")
        .expect("fresh write should record a revision");

    assert_eq!(revision.revision, 1);
    assert_eq!(revision.kind.as_str(), "fresh");
    assert_eq!(revision.file_count, 1);
    assert_eq!(revision.symbol_count, 1);
}

// ---------------------------------------------------------------------------
// Test 11: Workspace cleanup should clear canonical revision metadata
// ---------------------------------------------------------------------------
#[test]
fn test_delete_workspace_data_clears_canonical_revisions() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.incremental_update_atomic(
        &[],
        &[make_file("src/main.rs")],
        &[make_symbol("sym_a", "do_stuff", "src/main.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )
    .expect("incremental_update_atomic should succeed");

    let cleanup = db
        .delete_workspace_data()
        .expect("workspace cleanup should succeed");

    assert_eq!(cleanup.files_deleted, 1);
    assert_eq!(cleanup.symbols_deleted, 1);
    assert_eq!(cleanup.revisions_deleted, 1);
    assert_eq!(
        db.get_latest_canonical_revision("ws_test")
            .expect("revision lookup should succeed"),
        None
    );

    let usage = db
        .get_workspace_usage_stats("ws_test")
        .expect("workspace usage stats should succeed");
    assert_eq!(usage.symbol_count, 0);
    assert_eq!(usage.file_count, 0);
    assert_eq!(usage.canonical_revision, None);
}

// ---------------------------------------------------------------------------
// Test 12 (B-I3): delete_workspace_data must clear every owned table,
// not just symbols/files/revisions. Orphan rows in symbol_vectors, identifiers,
// types, and indexing_repairs were previously left behind.
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Type arguments (Miller bridge Phase 2): re-index must clean stale rows
// ---------------------------------------------------------------------------
#[test]
fn test_incremental_update_atomic_cleans_and_replaces_type_arguments() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // --- Round 1: file_a.rs has a nested generic use site
    //     Dictionary<string, List<int>> on identifier `id_dict`.
    //     Flattens to 3 rows: string (0), List (1), int (1.0, child of List).
    let files_v1 = vec![make_file("file_a.rs")];
    let idents_v1 = vec![make_identifier("id_dict", "Dictionary", "file_a.rs")];
    let rows_v1 = type_argument_rows(
        "id_dict",
        "file_a.rs",
        vec![
            leaf_arg(0, "string"),
            TypeArgument {
                ordinal: 1,
                type_name: "List".to_string(),
                children: vec![leaf_arg(0, "int")],
            },
        ],
    );
    let write_set_v1 = CanonicalWriteSet {
        files: &files_v1,
        symbols: &[],
        relationships: &[],
        identifiers: &idents_v1,
        types: &[],
        type_arguments: &rows_v1,
        literals: &[],
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set_v1,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("round 1 type-argument write should succeed");

    assert_eq!(
        count_rows(&db, "type_arguments"),
        3,
        "Dictionary<string, List<int>> flattens to 3 type-argument rows"
    );

    // The nested `int` row links to the `List` row via parent_arg_id.
    let list_id: String = db
        .conn
        .query_row(
            "SELECT id FROM type_arguments WHERE type_name = 'List'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let int_parent: Option<String> = db
        .conn
        .query_row(
            "SELECT parent_arg_id FROM type_arguments WHERE type_name = 'int'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        int_parent.as_deref(),
        Some(list_id.as_str()),
        "nested int row must point at its List parent"
    );
    assert_eq!(
        count_rows_where(&db, "type_arguments", "identifier_id = 'id_dict'"),
        3,
        "every round-1 row belongs to the id_dict use site"
    );

    // --- Round 2: file_a.rs re-indexed with a simpler use site List<int> on
    //     identifier `id_list` (1 row). Cleaning file_a.rs must drop all 3
    //     stale rows before the new row is inserted — no orphans by file_path
    //     or by the dead identifier_id.
    let files_v2 = vec![make_file("file_a.rs")];
    let idents_v2 = vec![make_identifier("id_list", "List", "file_a.rs")];
    let rows_v2 = type_argument_rows("id_list", "file_a.rs", vec![leaf_arg(0, "int")]);
    let write_set_v2 = CanonicalWriteSet {
        files: &files_v2,
        symbols: &[],
        relationships: &[],
        identifiers: &idents_v2,
        types: &[],
        type_arguments: &rows_v2,
        literals: &[],
    };
    db.incremental_update_atomic_with_metadata(
        &["file_a.rs".to_string()],
        &write_set_v2,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("round 2 type-argument write should succeed");

    assert_eq!(
        count_rows(&db, "type_arguments"),
        1,
        "re-index must clean the 3 old rows and leave only the 1 new row"
    );
    assert_eq!(
        count_rows_where(&db, "type_arguments", "identifier_id = 'id_dict'"),
        0,
        "stale rows from the previous extraction must be gone (no orphans)"
    );
    let surviving: String = db
        .conn
        .query_row("SELECT identifier_id FROM type_arguments", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        surviving, "id_list",
        "the only surviving row must belong to the new use site"
    );
}

// ---------------------------------------------------------------------------
// Type arguments: a full-replace rebuild (replace_workspace_data_atomic) wipes
// every prior row, not just the rewritten file's.
// ---------------------------------------------------------------------------
#[test]
fn test_replace_workspace_data_atomic_clears_stale_type_arguments() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Seed two files' type-argument rows via an incremental write.
    let files_v1 = vec![make_file("a.rs"), make_file("b.rs")];
    let idents_v1 = vec![
        make_identifier("id_a", "List", "a.rs"),
        make_identifier("id_b", "List", "b.rs"),
    ];
    let mut rows_v1 = type_argument_rows("id_a", "a.rs", vec![leaf_arg(0, "int")]);
    rows_v1.extend(type_argument_rows(
        "id_b",
        "b.rs",
        vec![leaf_arg(0, "string")],
    ));
    let write_set_v1 = CanonicalWriteSet {
        files: &files_v1,
        symbols: &[],
        relationships: &[],
        identifiers: &idents_v1,
        types: &[],
        type_arguments: &rows_v1,
        literals: &[],
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set_v1,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("seed write should succeed");
    assert_eq!(
        count_rows(&db, "type_arguments"),
        2,
        "precondition: two seeded type-argument rows across two files"
    );

    // Full-replace rebuild with only one file's row. replace_workspace_data_atomic
    // wipes ALL indexed rows first (delete_all_indexed_rows_tx), so b.rs's row
    // must vanish even though b.rs is absent from the rebuild batch.
    let files_v2 = vec![make_file("a.rs")];
    let idents_v2 = vec![make_identifier("id_a2", "Span", "a.rs")];
    let rows_v2 = type_argument_rows("id_a2", "a.rs", vec![leaf_arg(0, "byte")]);
    let write_set_v2 = CanonicalWriteSet {
        files: &files_v2,
        symbols: &[],
        relationships: &[],
        identifiers: &idents_v2,
        types: &[],
        type_arguments: &rows_v2,
        literals: &[],
    };
    db.replace_workspace_data_atomic(
        &write_set_v2,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("full-replace rebuild should succeed");

    assert_eq!(
        count_rows(&db, "type_arguments"),
        1,
        "full-replace rebuild must clear every prior type-argument row, not just the rewritten file's"
    );
    let surviving: String = db
        .conn
        .query_row("SELECT type_name FROM type_arguments", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        surviving, "byte",
        "only the rebuilt batch's row may survive a full-replace rebuild"
    );
}

#[test]
fn test_delete_workspace_data_clears_all_owned_tables() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file("src/lib.rs")];
    let symbols = vec![
        make_symbol("sym_a", "do_stuff", "src/lib.rs"),
        make_symbol("sym_b", "helper", "src/lib.rs"),
    ];
    let relationships = vec![make_relationship("rel_1", "sym_a", "sym_b", "src/lib.rs")];
    let identifiers = vec![make_identifier("ident_1", "helper", "src/lib.rs")];
    let types = vec![make_type_info("sym_a", "Result<(), Error>")];

    db.incremental_update_atomic(
        &[],
        &files,
        &symbols,
        &relationships,
        &identifiers,
        &types,
        "ws_test",
    )
    .expect("incremental_update_atomic should succeed");

    db.store_embeddings(&[("sym_a".to_string(), vec![0.1f32; 384])])
        .expect("store_embeddings should succeed");

    db.record_indexing_repair("src/lib.rs", "tantivy_dirty", Some("test"))
        .expect("record_indexing_repair should succeed");

    // Seed a type-argument row so workspace cleanup is verified to clear it too.
    let ta_idents = vec![make_identifier("id_ta", "List", "src/lib.rs")];
    let ta_rows = type_argument_rows("id_ta", "src/lib.rs", vec![leaf_arg(0, "int")]);
    let ta_write_set = CanonicalWriteSet {
        files: &[],
        symbols: &[],
        relationships: &[],
        identifiers: &ta_idents,
        types: &[],
        type_arguments: &ta_rows,
        literals: &[],
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &ta_write_set,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("seeding type_arguments should succeed");

    // Seed a literal row so workspace cleanup is verified to clear it too.
    let lit_rows = vec![make_literal(
        "lit_ws",
        "/api/health",
        LiteralKind::Url,
        "fetch",
        "src/lib.rs",
    )];
    let lit_write_set = CanonicalWriteSet {
        files: &[],
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &lit_rows,
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &lit_write_set,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("seeding literals should succeed");

    assert!(count_rows(&db, "symbols") > 0, "precondition: symbols");
    assert!(count_rows(&db, "files") > 0, "precondition: files");
    assert!(
        count_rows(&db, "identifiers") > 0,
        "precondition: identifiers"
    );
    assert!(count_rows(&db, "types") > 0, "precondition: types");
    assert!(
        count_rows(&db, "symbol_vectors") > 0,
        "precondition: symbol_vectors"
    );
    assert!(
        count_rows(&db, "indexing_repairs") > 0,
        "precondition: indexing_repairs"
    );
    assert!(
        count_rows(&db, "canonical_revisions") > 0,
        "precondition: canonical_revisions"
    );
    assert!(
        count_rows(&db, "type_arguments") > 0,
        "precondition: type_arguments"
    );
    assert!(count_rows(&db, "literals") > 0, "precondition: literals");

    db.delete_workspace_data()
        .expect("workspace cleanup should succeed");

    assert_eq!(count_rows(&db, "symbols"), 0, "symbols must be cleared");
    assert_eq!(count_rows(&db, "files"), 0, "files must be cleared");
    assert_eq!(
        count_rows(&db, "relationships"),
        0,
        "relationships must be cleared"
    );
    assert_eq!(
        count_rows(&db, "identifiers"),
        0,
        "identifiers must be cleared"
    );
    assert_eq!(count_rows(&db, "types"), 0, "types must be cleared");
    assert_eq!(
        count_rows(&db, "symbol_vectors"),
        0,
        "symbol_vectors must be cleared"
    );
    assert_eq!(
        count_rows(&db, "indexing_repairs"),
        0,
        "indexing_repairs must be cleared"
    );
    assert_eq!(
        count_rows(&db, "canonical_revisions"),
        0,
        "canonical_revisions must be cleared"
    );
    assert_eq!(
        count_rows(&db, "projection_states"),
        0,
        "projection_states must be cleared"
    );
    assert_eq!(
        count_rows(&db, "type_arguments"),
        0,
        "type_arguments must be cleared"
    );
    assert_eq!(count_rows(&db, "literals"), 0, "literals must be cleared");
}

// ---------------------------------------------------------------------------
// Literals (Miller bridge Phase 3): a stored roundtrip, per-file re-index
// cleanup, and full-replace wipe — mirroring the type_arguments coverage above.
// ---------------------------------------------------------------------------

#[test]
fn test_literals_roundtrip_persists_all_columns() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files = vec![make_file("api.ts")];
    let literals = vec![make_literal(
        "lit_url",
        "/api/users/{}",
        LiteralKind::Url,
        "fetch",
        "api.ts",
    )];
    let write_set = CanonicalWriteSet {
        files: &files,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals,
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("literal write should succeed");

    // Read every persisted column back and assert the row roundtrips intact.
    let (text, kind, carrier, arg_position, language, file_path): (
        String,
        String,
        Option<String>,
        i64,
        String,
        String,
    ) = db
        .conn
        .query_row(
            "SELECT literal_text, kind, carrier, arg_position, language, file_path \
             FROM literals WHERE id = 'lit_url'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("the literal row must be readable");
    assert_eq!(text, "/api/users/{}", "decoded text must roundtrip");
    assert_eq!(kind, "url", "kind must persist as its db string");
    assert_eq!(carrier.as_deref(), Some("fetch"), "carrier must persist");
    assert_eq!(arg_position, 0);
    assert_eq!(language, "rust");
    assert_eq!(file_path, "api.ts");
}

#[test]
fn test_incremental_update_atomic_cleans_and_replaces_literals() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // --- Round 1: api.ts has two url literals.
    let files_v1 = vec![make_file("api.ts")];
    let literals_v1 = vec![
        make_literal("lit_a", "/api/users", LiteralKind::Url, "fetch", "api.ts"),
        make_literal(
            "lit_b",
            "/api/orders",
            LiteralKind::Url,
            "axios.get",
            "api.ts",
        ),
    ];
    let write_set_v1 = CanonicalWriteSet {
        files: &files_v1,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals_v1,
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set_v1,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("round 1 literal write should succeed");
    assert_eq!(
        count_rows(&db, "literals"),
        2,
        "two round-1 literals stored"
    );

    // --- Round 2: api.ts re-indexed with a single different literal. Cleaning
    //     api.ts must drop BOTH stale rows before the new row is inserted — no
    //     orphans by file_path (the gate is off during bulk writes, so the FK
    //     CASCADE never fires; the explicit DELETE must handle it).
    let files_v2 = vec![make_file("api.ts")];
    let literals_v2 = vec![make_literal(
        "lit_c",
        "/api/products",
        LiteralKind::Url,
        "fetch",
        "api.ts",
    )];
    let write_set_v2 = CanonicalWriteSet {
        files: &files_v2,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals_v2,
    };
    db.incremental_update_atomic_with_metadata(
        &["api.ts".to_string()],
        &write_set_v2,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("round 2 literal write should succeed");

    assert_eq!(
        count_rows(&db, "literals"),
        1,
        "re-index must clean the 2 stale literals and leave only the 1 new row"
    );
    let surviving: String = db
        .conn
        .query_row("SELECT literal_text FROM literals", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        surviving, "/api/products",
        "only the re-indexed literal may survive"
    );
}

#[test]
fn test_replace_workspace_data_atomic_clears_stale_literals() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let files_v1 = vec![make_file("a.ts"), make_file("b.ts")];
    let literals_v1 = vec![
        make_literal("la", "/a", LiteralKind::Url, "fetch", "a.ts"),
        make_literal("lb", "/b", LiteralKind::Url, "fetch", "b.ts"),
    ];
    let write_set_v1 = CanonicalWriteSet {
        files: &files_v1,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals_v1,
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set_v1,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("seed write should succeed");
    assert_eq!(count_rows(&db, "literals"), 2, "precondition: two literals");

    let files_v2 = vec![make_file("a.ts")];
    let literals_v2 = vec![make_literal(
        "la2",
        "/a2",
        LiteralKind::Url,
        "fetch",
        "a.ts",
    )];
    let write_set_v2 = CanonicalWriteSet {
        files: &files_v2,
        symbols: &[],
        relationships: &[],
        identifiers: &[],
        types: &[],
        type_arguments: &[],
        literals: &literals_v2,
    };
    db.replace_workspace_data_atomic(
        &write_set_v2,
        "ws_test",
        AtomicPersistenceMetadata::default(),
    )
    .expect("full-replace rebuild should succeed");

    assert_eq!(
        count_rows(&db, "literals"),
        1,
        "full-replace rebuild must clear every prior literal, not just the rewritten file's"
    );
    let surviving: String = db
        .conn
        .query_row("SELECT literal_text FROM literals", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        surviving, "/a2",
        "only the rebuilt batch's literal may survive"
    );
}
