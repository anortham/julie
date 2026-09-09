use std::path::Path;
use tempfile::tempdir;

use crate::database::{LATEST_SCHEMA_VERSION, SymbolDatabase};
use crate::test_support::{file_info_builder, identifier_builder, open_test_connection};

fn build_v30_database(db_path: &Path) {
    let conn = open_test_connection(db_path).unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         CREATE TABLE schema_version (
             version INTEGER PRIMARY KEY,
             applied_at INTEGER NOT NULL,
             description TEXT NOT NULL
         );",
    )
    .unwrap();

    for v in 1..=30 {
        conn.execute(
            "INSERT INTO schema_version (version, applied_at, description) VALUES (?1, 1000, ?2)",
            rusqlite::params![v, format!("Migration {v}")],
        )
        .unwrap();
    }

    conn.execute_batch(
        "CREATE TABLE files (
             path TEXT PRIMARY KEY,
             language TEXT NOT NULL,
             hash TEXT NOT NULL,
             size INTEGER NOT NULL,
             last_modified INTEGER NOT NULL,
             last_indexed INTEGER DEFAULT 0,
             parse_cache BLOB,
             symbol_count INTEGER DEFAULT 0,
             content TEXT,
             line_count INTEGER DEFAULT 0,
             workspace_id TEXT NOT NULL DEFAULT 'primary'
         );
         CREATE TABLE symbols (
             id TEXT PRIMARY KEY,
             name TEXT NOT NULL,
             kind TEXT NOT NULL,
             language TEXT NOT NULL,
             file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
             start_line INTEGER NOT NULL,
             start_col INTEGER NOT NULL,
             end_line INTEGER NOT NULL,
             end_col INTEGER NOT NULL,
             start_byte INTEGER,
             end_byte INTEGER,
             signature TEXT,
             doc_comment TEXT,
             visibility TEXT,
             parent_id TEXT,
             metadata TEXT,
             last_indexed INTEGER DEFAULT 0,
             content_type TEXT,
             reference_score REAL DEFAULT 0.0,
             semantic_group TEXT,
             confidence REAL,
             body_span TEXT,
             body_hash TEXT
         );
         CREATE TABLE identifiers (
             id TEXT PRIMARY KEY,
             name TEXT NOT NULL,
             kind TEXT NOT NULL,
             language TEXT NOT NULL,
             file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
             start_line INTEGER NOT NULL,
             start_col INTEGER NOT NULL,
             end_line INTEGER NOT NULL,
             end_col INTEGER NOT NULL,
             start_byte INTEGER,
             end_byte INTEGER,
             containing_symbol_id TEXT REFERENCES symbols(id) ON DELETE CASCADE,
             target_symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,
             confidence REAL DEFAULT 1.0,
             code_context TEXT,
             last_indexed INTEGER DEFAULT 0
         );
         INSERT INTO files (path, language, hash, size, last_modified)
         VALUES ('src/test.rs', 'rust', 'hash-1', 100, 1000);
         INSERT INTO identifiers (
             id, name, kind, language, file_path, start_line, start_col,
             end_line, end_col, start_byte, end_byte, confidence, code_context
         ) VALUES (
             'id-pre-migration', 'render', 'call', 'rust', 'src/test.rs',
             10, 4, 10, 10, 100, 106, 0.9, 'x.render()'
         );",
    )
    .unwrap();
}

#[test]
fn receiver_type_column_is_present_after_open_and_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("symbols.db");
    for _ in 0..2 {
        let db = SymbolDatabase::new(&path).unwrap();
        assert!(db.has_column("identifiers", "receiver_type").unwrap());
    }
}

#[test]
fn fresh_database_creation_initializes_schema_31_with_receiver_type() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("fresh.db");
    let db = SymbolDatabase::new(&path).unwrap();
    assert_eq!(db.get_schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    assert_eq!(LATEST_SCHEMA_VERSION, 32);
    assert!(db.has_column("identifiers", "receiver_type").unwrap());
}

#[test]
fn migration_031_upgrades_v30_database_without_data_loss() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("v30.db");
    build_v30_database(&path);

    {
        let pre_check = open_test_connection(&path).unwrap();
        let has_col: bool = pre_check
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('identifiers') WHERE name = 'receiver_type'",
                [],
                |row| {
                    let count: i32 = row.get(0)?;
                    Ok(count > 0)
                },
            )
            .unwrap();
        assert!(!has_col);
    }

    let db = SymbolDatabase::new(&path).unwrap();
    assert_eq!(db.get_schema_version().unwrap(), 32);
    assert!(db.has_column("identifiers", "receiver_type").unwrap());

    let (id, name, kind, receiver_type): (String, String, String, Option<String>) = db
        .conn
        .query_row(
            "SELECT id, name, kind, receiver_type FROM identifiers WHERE id = 'id-pre-migration'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(id, "id-pre-migration");
    assert_eq!(name, "render");
    assert_eq!(kind, "call");
    assert_eq!(receiver_type, None);

    let refs = db
        .get_identifiers_by_names(&["render".to_string()])
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].name, "render");
    assert_eq!(refs[0].receiver_type, None);

    assert!(db.migration_031_add_identifier_receiver_type().is_ok());
}

#[test]
fn bulk_identifier_insertion_preserves_some_and_none_receiver_types() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("bulk_identifiers.db");
    let mut db = SymbolDatabase::new(&path).unwrap();

    let file = file_info_builder("src/service.rs").build();
    db.store_file_info(&file).unwrap();

    let id_with_type = identifier_builder("id-typed", "execute", "src/service.rs")
        .receiver_type("OrderService")
        .build();
    let id_without_type = identifier_builder("id-untyped", "execute", "src/service.rs").build();

    db.bulk_store_identifiers(&[id_with_type, id_without_type], "primary")
        .unwrap();

    let refs = db
        .get_identifiers_by_names(&["execute".to_string()])
        .unwrap();
    assert_eq!(refs.len(), 2);

    let typed_ref = refs.iter().find(|r| r.receiver_type.is_some()).unwrap();
    assert_eq!(typed_ref.receiver_type.as_deref(), Some("OrderService"));

    let untyped_ref = refs.iter().find(|r| r.receiver_type.is_none()).unwrap();
    assert_eq!(untyped_ref.receiver_type, None);
}
