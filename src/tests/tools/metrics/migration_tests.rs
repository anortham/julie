use crate::database::SymbolDatabase;
use tempfile::TempDir;

#[test]
fn test_migration_013_creates_tool_calls_table() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    let count: i32 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tool_calls'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "tool_calls table should exist after migration");
}

#[test]
fn test_migration_013_tool_calls_has_expected_columns() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    let col_count: i32 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('tool_calls')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(col_count, 10, "tool_calls should have 10 columns (id + 9 data columns)");
}

#[test]
fn test_migration_013_adds_line_count_to_files() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    assert!(
        db.has_column("files", "line_count").unwrap(),
        "files table should have line_count column"
    );
}

#[test]
fn test_migration_013_tool_calls_indexes_exist() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    let indexes: Vec<String> = {
        let mut stmt = db
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='tool_calls'")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert!(indexes.iter().any(|n| n == "idx_tool_calls_timestamp"));
    assert!(indexes.iter().any(|n| n == "idx_tool_calls_tool_name"));
    assert!(indexes.iter().any(|n| n == "idx_tool_calls_session"));
}
