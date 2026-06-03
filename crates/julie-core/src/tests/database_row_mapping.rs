use crate::database::SymbolDatabase;
use crate::test_support::{file_info_builder, store_file_info_if_missing, symbol_builder};
use tempfile::TempDir;

#[test]
fn test_db_row_to_symbol_unknown_kind_does_not_silently_coerce() {
    let (_temp_dir, db) = test_db();
    insert_file(&db);
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('bad-kind', 'BadKind', 'not_a_symbol_kind', 'rust', 'src/lib.rs', 1, 1, 0, 7, 0, 7)",
            [],
        )
        .unwrap();

    let err = db
        .get_symbol_by_id("bad-kind")
        .expect_err("unknown symbol kind should return a database error");

    assert!(
        err.to_string().contains("unknown symbol kind"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_db_row_to_symbol_unknown_visibility_does_not_silently_drop() {
    let (_temp_dir, db) = test_db();
    insert_file(&db);
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte, visibility)
             VALUES ('bad-visibility', 'BadVisibility', 'function', 'rust', 'src/lib.rs', 1, 1, 0, 13, 0, 13, 'package_private')",
            [],
        )
        .unwrap();

    let err = db
        .get_symbol_by_id("bad-visibility")
        .expect_err("unknown visibility should return a database error");

    assert!(
        err.to_string().contains("unknown symbol visibility"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_db_row_to_relationship_unknown_kind_does_not_silently_coerce() {
    let (_temp_dir, mut db) = test_db();
    insert_file(&db);
    insert_symbol(&mut db, "source", "Source");
    insert_symbol(&mut db, "target", "Target");
    db.conn
        .execute(
            "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
             VALUES ('bad-rel', 'source', 'target', 'not_a_relationship_kind', 'src/lib.rs', 1)",
            [],
        )
        .unwrap();

    let err = db
        .get_outgoing_relationships("source")
        .expect_err("unknown relationship kind should return a database error");

    assert!(
        err.to_string().contains("unknown relationship kind"),
        "unexpected error: {err}"
    );
}

fn test_db() -> (TempDir, SymbolDatabase) {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("symbols.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (temp_dir, db)
}

fn insert_file(db: &SymbolDatabase) {
    store_file_info_if_missing(
        db,
        &file_info_builder("src/lib.rs")
            .hash("hash")
            .size(0)
            .last_modified(0)
            .symbol_count(0)
            .line_count(0)
            .build(),
    )
    .unwrap();
}

fn insert_symbol(db: &mut SymbolDatabase, id: &str, name: &str) {
    db.store_symbols(&[symbol_builder(id, name, "src/lib.rs")
        .span(1, 0, 1, 5)
        .bytes(0, 5)
        .confidence(1.0)
        .build()])
        .unwrap();
}
