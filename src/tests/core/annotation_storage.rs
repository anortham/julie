use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{AnnotationMarker, Symbol, SymbolKind};
use rusqlite::Connection;
use tempfile::TempDir;

fn file_info(path: &str, language: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: language.to_string(),
        hash: format!("hash-{path}"),
        size: 128,
        last_modified: 1_700_000_000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 12,
        content: Some(format!("// {path}")),
    }
}

fn marker(
    annotation: &str,
    annotation_key: &str,
    raw_text: Option<&str>,
    carrier: Option<&str>,
) -> AnnotationMarker {
    AnnotationMarker {
        annotation: annotation.to_string(),
        annotation_key: annotation_key.to_string(),
        raw_text: raw_text.map(str::to_string),
        carrier: carrier.map(str::to_string),
    }
}

fn symbol(id: &str, name: &str, file_path: &str, annotations: Vec<AnnotationMarker>) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 3,
        start_column: 4,
        end_line: 8,
        end_column: 1,
        start_byte: 20,
        end_byte: 80,
        signature: Some(format!("fn {name}()")),
        doc_comment: Some(format!("/// docs for {name}")),
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: Some("annotation-storage".to_string()),
        confidence: Some(0.98),
        code_context: Some(format!("fn {name}() {{}}")),
        content_type: None,
        annotations,
    }
}

fn open_db() -> (TempDir, SymbolDatabase) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("annotations.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (temp_dir, db)
}

fn annotation_indexes(db: &SymbolDatabase) -> Vec<String> {
    let mut stmt = db
        .conn
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'index' AND tbl_name = 'symbol_annotations'
             ORDER BY name",
        )
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn annotation_row_count(db: &SymbolDatabase, symbol_id: &str) -> i64 {
    db.conn
        .query_row(
            "SELECT COUNT(*) FROM symbol_annotations WHERE symbol_id = ?1",
            [symbol_id],
            |row| row.get(0),
        )
        .unwrap()
}

fn all_annotation_row_count(db: &SymbolDatabase) -> i64 {
    db.conn
        .query_row("SELECT COUNT(*) FROM symbol_annotations", [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn rich_markers() -> Vec<AnnotationMarker> {
    vec![
        marker("Serialize", "serialize", Some("Serialize"), Some("derive")),
        marker(
            "serde::Deserialize",
            "serde::deserialize",
            Some("Deserialize"),
            None,
        ),
    ]
}

#[test]
fn new_database_creates_symbol_annotations_with_indexes() {
    let (_temp_dir, db) = open_db();

    let table_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name = 'symbol_annotations'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let indexes = annotation_indexes(&db);

    assert_eq!(table_count, 1);
    assert!(
        indexes
            .iter()
            .any(|name| name == "idx_symbol_annotations_annotation_key")
    );
    assert!(
        indexes
            .iter()
            .any(|name| name == "idx_symbol_annotations_carrier")
    );
}

#[test]
fn migration_020_creates_symbol_annotations_for_existing_database() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("migrated.db");

    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL,
                description TEXT NOT NULL
            );
            INSERT INTO schema_version (version, applied_at, description)
            VALUES (19, 1, 'Add revision_file_changes table');",
        )
        .unwrap();
    }

    let db = SymbolDatabase::new(&db_path).unwrap();
    let version = db.get_schema_version().unwrap();
    let indexes = annotation_indexes(&db);

    assert_eq!(version, crate::database::LATEST_SCHEMA_VERSION);
    assert!(
        indexes
            .iter()
            .any(|name| name == "idx_symbol_annotations_annotation_key")
    );
    assert!(
        indexes
            .iter()
            .any(|name| name == "idx_symbol_annotations_carrier")
    );
}

#[test]
fn store_symbols_hydrates_annotations_in_full_reads_and_keeps_lightweight_empty() {
    let (_temp_dir, mut db) = open_db();
    let file = file_info("src/annotated.rs", "rust");
    db.store_file_info(&file).unwrap();

    let expected = rich_markers();
    let stored = symbol("symbol-store", "annotated", &file.path, expected.clone());
    db.store_symbols(&[stored]).unwrap();

    let by_id = db.get_symbol_by_id("symbol-store").unwrap().unwrap();
    let for_file = db.get_symbols_for_file(&file.path).unwrap();
    let by_query_name = db.find_symbols_by_name("annotated").unwrap();
    let by_search_name = db.get_symbols_by_name("annotated").unwrap();
    let all = db.get_all_symbols().unwrap();
    let lightweight = db.get_symbols_for_file_lightweight(&file.path).unwrap();

    assert_eq!(by_id.annotations, expected);
    assert_eq!(for_file[0].annotations, expected);
    assert_eq!(by_query_name[0].annotations, expected);
    assert_eq!(by_search_name[0].annotations, expected);
    assert_eq!(all[0].annotations, expected);
    assert!(lightweight[0].annotations.is_empty());
}

#[test]
fn store_symbols_transactional_replaces_annotation_rows() {
    let (_temp_dir, mut db) = open_db();
    let file = file_info("src/replace.rs", "rust");
    db.store_file_info(&file).unwrap();

    let with_annotations = symbol("symbol-replace", "replace_me", &file.path, rich_markers());
    db.store_symbols_transactional(&[with_annotations]).unwrap();
    assert_eq!(annotation_row_count(&db, "symbol-replace"), 2);

    let without_annotations = symbol("symbol-replace", "replace_me", &file.path, Vec::new());
    db.store_symbols_transactional(&[without_annotations])
        .unwrap();

    let reloaded = db.get_symbol_by_id("symbol-replace").unwrap().unwrap();
    assert!(reloaded.annotations.is_empty());
    assert_eq!(annotation_row_count(&db, "symbol-replace"), 0);
}

#[test]
fn delete_symbols_for_file_removes_annotation_rows() {
    let (_temp_dir, mut db) = open_db();
    let file = file_info("src/delete.rs", "rust");
    db.store_file_info(&file).unwrap();
    let stored = symbol("symbol-delete", "delete_me", &file.path, rich_markers());
    db.store_symbols_transactional(&[stored]).unwrap();

    db.delete_symbols_for_file(&file.path).unwrap();

    assert_eq!(db.get_symbol_by_id("symbol-delete").unwrap(), None);
    assert_eq!(annotation_row_count(&db, "symbol-delete"), 0);
}

#[test]
fn bulk_store_symbols_persists_annotations() {
    let (_temp_dir, mut db) = open_db();
    let stored = symbol(
        "symbol-bulk",
        "bulk_annotated",
        "src/bulk.rs",
        rich_markers(),
    );

    db.bulk_store_symbols(&[stored], "primary").unwrap();

    let reloaded = db.get_symbol_by_id("symbol-bulk").unwrap().unwrap();
    assert_eq!(reloaded.annotations, rich_markers());
}

#[test]
fn incremental_update_persists_annotations_and_cleans_rows_with_foreign_keys_disabled() {
    let (_temp_dir, mut db) = open_db();
    let file = file_info("src/incremental.rs", "rust");
    let stored = symbol(
        "symbol-incremental",
        "incremental_annotated",
        &file.path,
        rich_markers(),
    );

    db.incremental_update_atomic(&[], &[file.clone()], &[stored], &[], &[], &[], "primary")
        .unwrap();
    assert_eq!(annotation_row_count(&db, "symbol-incremental"), 2);

    db.incremental_update_atomic(&[file.path.clone()], &[], &[], &[], &[], &[], "primary")
        .unwrap();

    assert_eq!(db.get_symbol_by_id("symbol-incremental").unwrap(), None);
    assert_eq!(all_annotation_row_count(&db), 0);
}

#[test]
fn bulk_store_fresh_atomic_persists_annotations() {
    let (_temp_dir, mut db) = open_db();
    let file = file_info("src/fresh.rs", "rust");
    let stored = symbol(
        "symbol-fresh",
        "fresh_annotated",
        &file.path,
        rich_markers(),
    );

    db.bulk_store_fresh_atomic(&[file], &[stored], &[], &[], &[], "primary")
        .unwrap();

    let reloaded = db.get_symbol_by_id("symbol-fresh").unwrap().unwrap();
    assert_eq!(reloaded.annotations, rich_markers());
}
