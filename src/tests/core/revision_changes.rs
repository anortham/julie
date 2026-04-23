use crate::database::SymbolDatabase;
use crate::database::types::FileInfo;
use crate::extractors::{Symbol, SymbolKind};
use tempfile::TempDir;

fn make_file(path: &str, hash: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: hash.to_string(),
        size: 128,
        last_modified: 1_700_000_000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 10,
        content: None,
    }
}

fn make_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        end_line: 3,
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 42,
        parent_id: None,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        visibility: None,
        metadata: None,
        semantic_group: None,
        confidence: Some(1.0),
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    }
}

fn setup_db() -> (TempDir, SymbolDatabase) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (temp_dir, db)
}

#[test]
fn test_incremental_update_records_revision_file_changes() {
    let (_tmp, mut db) = setup_db();

    db.incremental_update_atomic(
        &[],
        &[make_file("src/added.rs", "hash_added_v1")],
        &[make_symbol("sym_added", "added_fn", "src/added.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )
    .expect("initial incremental write should succeed");

    db.incremental_update_atomic(
        &["src/added.rs".to_string()],
        &[
            make_file("src/added.rs", "hash_added_v2"),
            make_file("src/new.rs", "hash_new_v1"),
        ],
        &[
            make_symbol("sym_added_v2", "added_fn", "src/added.rs"),
            make_symbol("sym_new", "new_fn", "src/new.rs"),
        ],
        &[],
        &[],
        &[],
        "ws_test",
    )
    .expect("second incremental write should succeed");

    let revision = db
        .get_current_canonical_revision("ws_test")
        .expect("current revision lookup should succeed")
        .expect("second write should record a canonical revision");

    let mut stmt = db
        .conn
        .prepare(
            "SELECT file_path, change_kind, old_hash, new_hash
             FROM revision_file_changes
             WHERE revision = ?1
             ORDER BY file_path",
        )
        .expect("revision_file_changes query should prepare");

    let rows: Vec<(String, String, Option<String>, Option<String>)> = stmt
        .query_map([revision], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .expect("revision_file_changes query should run")
        .collect::<Result<_, _>>()
        .expect("revision_file_changes rows should decode");

    assert_eq!(
        rows,
        vec![
            (
                "src/added.rs".to_string(),
                "modified".to_string(),
                Some("hash_added_v1".to_string()),
                Some("hash_added_v2".to_string()),
            ),
            (
                "src/new.rs".to_string(),
                "added".to_string(),
                None,
                Some("hash_new_v1".to_string()),
            ),
        ]
    );
}

#[test]
fn test_delete_orphaned_files_atomic_records_deleted_revision_file_changes() {
    let (_tmp, mut db) = setup_db();

    db.incremental_update_atomic(
        &[],
        &[make_file("src/orphan.rs", "hash_orphan_v1")],
        &[make_symbol("sym_orphan", "orphan_fn", "src/orphan.rs")],
        &[],
        &[],
        &[],
        "ws_test",
    )
    .expect("seed write should succeed");

    let revision = db
        .delete_orphaned_files_atomic("ws_test", &["src/orphan.rs".to_string()])
        .expect("delete_orphaned_files_atomic should succeed")
        .expect("orphan cleanup should record a revision");

    let row: (String, String, Option<String>, Option<String>) = db
        .conn
        .query_row(
            "SELECT file_path, change_kind, old_hash, new_hash
             FROM revision_file_changes
             WHERE revision = ?1",
            [revision],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("deleted revision_file_changes row should exist");

    assert_eq!(
        row,
        (
            "src/orphan.rs".to_string(),
            "deleted".to_string(),
            Some("hash_orphan_v1".to_string()),
            None,
        )
    );
}
