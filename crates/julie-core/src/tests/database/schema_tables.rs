use super::*;

const EXPECTED_TABLES: &[&str] = &[
    "workspaces",
    "canonical_revisions",
    "revision_file_changes",
    "projection_states",
    "index_engine_state",
    "files",
    "indexing_repairs",
    "symbols",
    "symbol_annotations",
    "early_warning_reports",
    "external_extract_metadata",
    "identifiers",
    "type_arguments",
    "literals",
    "source_regions",
    "structural_facts",
    "complexity_metrics",
    "web_edges",
    "types",
    "relationships",
    "schema_version",
    "embedding_config",
    "tool_calls",
];

fn table_names(db: &SymbolDatabase) -> Vec<String> {
    let mut stmt = db
        .conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn fresh_db(dir: &TempDir) -> SymbolDatabase {
    SymbolDatabase::new(dir.path().join("fresh.db")).unwrap()
}

#[test]
fn fresh_database_creates_every_schema_table() {
    let dir = TempDir::new().unwrap();
    let db = fresh_db(&dir);

    let names = table_names(&db);
    let missing: Vec<&str> = EXPECTED_TABLES
        .iter()
        .copied()
        .filter(|table| !names.iter().any(|name| name == table))
        .collect();
    assert!(missing.is_empty(), "missing tables: {missing:?}");
}

#[test]
fn fresh_database_records_latest_schema_version() {
    let dir = TempDir::new().unwrap();
    let db = fresh_db(&dir);

    assert_eq!(db.get_schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    assert!(db.schema_version_matches().unwrap());
}

#[test]
fn fresh_database_has_columns_added_late_in_schema_history() {
    let dir = TempDir::new().unwrap();
    let db = fresh_db(&dir);

    for (table, column) in [
        ("files", "content"),
        ("files", "line_count"),
        ("symbols", "reference_score"),
        ("symbols", "body_hash"),
        ("identifiers", "receiver_type"),
        ("tool_calls", "input_bytes"),
        ("embedding_config", "format_version"),
        ("projection_states", "projected_revision"),
    ] {
        assert!(db.has_column(table, column).unwrap(), "{table}.{column}");
    }
}

#[test]
fn reopening_an_out_of_date_database_neither_migrates_nor_alters_it() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("stale.db");
    let stale_version = LATEST_SCHEMA_VERSION - 1;
    {
        let _db = SymbolDatabase::new(&db_path).unwrap();
        let conn = open_test_connection(&db_path).unwrap();
        conn.execute("DELETE FROM schema_version", []).unwrap();
        conn.execute(
            "INSERT INTO schema_version (version, applied_at, description) VALUES (?1, 0, 'stale')",
            [stale_version],
        )
        .unwrap();
        conn.execute("DROP TABLE tool_calls", []).unwrap();
    }

    let db = SymbolDatabase::new(&db_path).unwrap();

    assert_eq!(db.get_schema_version().unwrap(), stale_version);
    assert!(!db.schema_version_matches().unwrap());
    assert!(!table_names(&db).iter().any(|name| name == "tool_calls"));
}

#[test]
fn newer_database_opens_and_reports_schema_mismatch() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("newer.db");
    {
        let _db = SymbolDatabase::new(&db_path).unwrap();
        let conn = open_test_connection(&db_path).unwrap();
        conn.execute(
            "INSERT INTO schema_version (version, applied_at, description) VALUES (?1, 0, 'future')",
            [LATEST_SCHEMA_VERSION + 10],
        )
        .unwrap();
    }

    let db = SymbolDatabase::new(&db_path).unwrap();
    assert!(!db.schema_version_matches().unwrap());
}

#[test]
fn index_engine_state_round_trip() {
    let dir = TempDir::new().unwrap();
    let db = fresh_db(&dir);

    db.set_index_engine_version("workspace-a", "semantic_index_engine", "version-a")
        .unwrap();

    assert_eq!(
        db.get_index_engine_version("workspace-a", "semantic_index_engine")
            .unwrap()
            .as_deref(),
        Some("version-a")
    );
    assert!(
        db.index_engine_version_matches("workspace-a", "semantic_index_engine", "version-a")
            .unwrap()
    );
    assert!(
        !db.index_engine_version_matches("workspace-a", "semantic_index_engine", "version-b")
            .unwrap()
    );
}
