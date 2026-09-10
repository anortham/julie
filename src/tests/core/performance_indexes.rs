use crate::database::SymbolDatabase;
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
        "idx_identifiers_name_kind_containing",
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
fn test_identifier_name_kind_container_index_exists() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("index_exists.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    let identifier_indexes = index_names(&db, "identifiers");
    assert!(
        identifier_indexes.contains(&"idx_identifiers_name_kind_containing".to_string()),
        "missing idx_identifiers_name_kind_containing: {identifier_indexes:?}"
    );
}
