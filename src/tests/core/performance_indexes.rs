use crate::database::{LATEST_SCHEMA_VERSION, SymbolDatabase};
use crate::tests::test_helpers::open_test_connection;
use tempfile::TempDir;

fn index_names(db: &SymbolDatabase, table: &str) -> Vec<String> {
    let mut stmt = db
        .conn
        .prepare(
            "SELECT name
             FROM sqlite_master
             WHERE type = 'index' AND tbl_name = ?1
             ORDER BY name",
        )
        .unwrap();

    stmt.query_map([table], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn fresh_database_creates_sql_performance_indexes() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("fresh.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    let relationship_indexes = index_names(&db, "relationships");
    assert!(
        relationship_indexes.contains(&"idx_rel_file".to_string()),
        "relationship file-path deletes need an index: {relationship_indexes:?}"
    );

    let identifier_indexes = index_names(&db, "identifiers");
    for expected in [
        "idx_identifiers_file_line_kind",
        "idx_identifiers_file_name",
        "idx_identifiers_kind_containing",
    ] {
        assert!(
            identifier_indexes.contains(&expected.to_string()),
            "missing {expected}: {identifier_indexes:?}"
        );
    }

    let symbol_indexes = index_names(&db, "symbols");
    assert!(
        symbol_indexes.contains(&"idx_symbols_reference_score_desc".to_string()),
        "centrality ranking needs reference_score index: {symbol_indexes:?}"
    );
}

#[test]
fn migration_adds_sql_performance_indexes_to_existing_database() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("legacy.db");

    {
        let conn = open_test_connection(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_version (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL,
                description TEXT NOT NULL
            );
            INSERT INTO schema_version (version, applied_at, description)
            VALUES (21, 0, 'legacy before performance indexes');",
        )
        .unwrap();
    }

    let db = SymbolDatabase::new(&db_path).unwrap();
    assert_eq!(db.get_schema_version().unwrap(), LATEST_SCHEMA_VERSION);

    assert!(
        index_names(&db, "relationships").contains(&"idx_rel_file".to_string()),
        "migration should add relationship file-path index"
    );

    let identifier_indexes = index_names(&db, "identifiers");
    assert!(
        identifier_indexes.contains(&"idx_identifiers_file_line_kind".to_string()),
        "migration should add identifier file-line-kind index"
    );
    assert!(
        identifier_indexes.contains(&"idx_identifiers_file_name".to_string()),
        "migration should add identifier file-name index"
    );
    assert!(
        identifier_indexes.contains(&"idx_identifiers_kind_containing".to_string()),
        "migration should add identifier kind-containing index"
    );

    assert!(
        index_names(&db, "symbols").contains(&"idx_symbols_reference_score_desc".to_string()),
        "migration should add reference-score index"
    );
}
