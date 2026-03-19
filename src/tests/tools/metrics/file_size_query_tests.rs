use crate::database::SymbolDatabase;
use crate::database::types::FileInfo;
use tempfile::TempDir;

fn test_db_with_files() -> (TempDir, SymbolDatabase) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(&FileInfo {
        path: "src/main.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc".to_string(),
        size: 1000,
        last_modified: 0,
        last_indexed: 0,
        symbol_count: 5,
        line_count: 50,
        content: None,
    })
    .unwrap();
    db.store_file_info(&FileInfo {
        path: "src/lib.rs".to_string(),
        language: "rust".to_string(),
        hash: "def".to_string(),
        size: 2000,
        last_modified: 0,
        last_indexed: 0,
        symbol_count: 10,
        line_count: 100,
        content: None,
    })
    .unwrap();
    db.store_file_info(&FileInfo {
        path: "src/utils.rs".to_string(),
        language: "rust".to_string(),
        hash: "ghi".to_string(),
        size: 500,
        last_modified: 0,
        last_indexed: 0,
        symbol_count: 3,
        line_count: 25,
        content: None,
    })
    .unwrap();

    (tmp, db)
}

#[test]
fn test_get_total_file_sizes_multiple() {
    let (_tmp, db) = test_db_with_files();
    let total = db
        .get_total_file_sizes(&["src/main.rs", "src/lib.rs"])
        .unwrap();
    assert_eq!(total, 3000);
}

#[test]
fn test_get_total_file_sizes_single() {
    let (_tmp, db) = test_db_with_files();
    let total = db.get_total_file_sizes(&["src/main.rs"]).unwrap();
    assert_eq!(total, 1000);
}

#[test]
fn test_get_total_file_sizes_all() {
    let (_tmp, db) = test_db_with_files();
    let total = db
        .get_total_file_sizes(&["src/main.rs", "src/lib.rs", "src/utils.rs"])
        .unwrap();
    assert_eq!(total, 3500);
}

#[test]
fn test_get_total_file_sizes_empty() {
    let (_tmp, db) = test_db_with_files();
    let total = db.get_total_file_sizes(&[]).unwrap();
    assert_eq!(total, 0);
}

#[test]
fn test_get_total_file_sizes_nonexistent_path() {
    let (_tmp, db) = test_db_with_files();
    let total = db.get_total_file_sizes(&["nonexistent.rs"]).unwrap();
    assert_eq!(total, 0);
}
