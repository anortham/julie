use super::*;

// ============================================================
// SCHEMA MIGRATION TESTS
// ============================================================

#[test]
fn test_migration_fresh_database_at_latest_version() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Fresh database should be at latest version
    let version = db.get_schema_version().unwrap();
    assert_eq!(version, LATEST_SCHEMA_VERSION);
}

#[test]
fn test_migration_version_table_exists() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify schema_version table exists
    let result: Result<i64, rusqlite::Error> =
        db.conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0));

    assert!(result.is_ok(), "schema_version table should exist");
}

#[test]
fn test_migration_024_index_engine_state_round_trip() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    db.set_index_engine_version("workspace-a", "semantic_index_engine", "version-a")
        .unwrap();

    let stored = db
        .get_index_engine_version("workspace-a", "semantic_index_engine")
        .unwrap();
    assert_eq!(stored.as_deref(), Some("version-a"));
    assert!(
        db.index_engine_version_matches("workspace-a", "semantic_index_engine", "version-a")
            .unwrap()
    );
    assert!(
        !db.index_engine_version_matches("workspace-a", "semantic_index_engine", "version-b")
            .unwrap()
    );
}

#[test]
fn test_migration_adds_content_column() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify content column exists in files table
    let has_content = db.has_column("files", "content").unwrap();
    assert!(
        has_content,
        "files table should have content column after migration"
    );
}

#[test]
fn test_migration_from_legacy_v1_database() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create a legacy V1 database (without content column)
    {
        let conn = open_test_connection(&db_path).unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        // Create old schema WITHOUT content column
        conn.execute(
            "CREATE TABLE files (
                path TEXT PRIMARY KEY,
                language TEXT NOT NULL,
                hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                last_modified INTEGER NOT NULL,
                last_indexed INTEGER DEFAULT 0,
                parse_cache BLOB,
                symbol_count INTEGER DEFAULT 0,
                workspace_id TEXT NOT NULL DEFAULT 'primary'
            )",
            [],
        )
        .unwrap();

        // Insert test data
        conn.execute(
            "INSERT INTO files (path, language, hash, size, last_modified)
             VALUES ('test.rs', 'rust', 'abc123', 1024, 1234567890)",
            [],
        )
        .unwrap();
    }

    // Now open with new code - should trigger migration
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify migration occurred
    let version = db.get_schema_version().unwrap();
    assert_eq!(
        version, LATEST_SCHEMA_VERSION,
        "Database should be migrated to latest version"
    );

    // Verify content column exists
    let has_content = db.has_column("files", "content").unwrap();
    assert!(has_content, "Migration should have added content column");

    // Verify existing data is preserved
    let file_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'test.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        file_count, 1,
        "Existing data should be preserved after migration"
    );
}

#[test]
fn test_migration_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create database (runs migrations)
    {
        let _db = SymbolDatabase::new(&db_path).unwrap();
    }

    // Open again (should handle already-migrated database)
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();
    let version = db.get_schema_version().unwrap();
    assert_eq!(version, LATEST_SCHEMA_VERSION);

    // Should not error or change version
    let has_content = db.has_column("files", "content").unwrap();
    assert!(has_content);
}
// 🚨 SCHEMA VERSION SAFETY: Test that newer schema is detected
#[test]
fn test_schema_version_downgrade_detection() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("schema_test.db");

    // Create database with current schema
    {
        let db = SymbolDatabase::new(&db_path).unwrap();
        let version = db.get_schema_version().unwrap();
        assert_eq!(version, crate::database::LATEST_SCHEMA_VERSION);
    }

    // Manually bump schema version to simulate newer database
    {
        use rusqlite::Connection;
        let conn = Connection::open(&db_path).unwrap();
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
            .unwrap();

        let future_version = crate::database::LATEST_SCHEMA_VERSION + 10;
        conn.execute(
            "INSERT OR REPLACE INTO schema_version (version, applied_at, description)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![future_version, 1234567890, "Future schema"],
        )
        .unwrap();
    }

    // Try to open with old code - should fail with clear error
    let result = SymbolDatabase::new(&db_path);
    assert!(
        result.is_err(),
        "Should fail when database schema is newer than code expects"
    );

    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("NEWER than code expects"),
            "Error should explain schema version mismatch clearly. Got: {}",
            error_msg
        );
        println!("✅ Schema version downgrade detection working");
    }
}
// ============================================================
// MIGRATION 009 & REFERENCE SCORE TESTS
// ============================================================

/// Task 1: Verify migration 009 adds reference_score column with DEFAULT 0.0
#[test]
fn test_migration_009_reference_score_column_exists() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify reference_score column exists in symbols table
    let has_col = db.has_column("symbols", "reference_score").unwrap();
    assert!(
        has_col,
        "symbols table should have reference_score column after migration 009"
    );
}

/// Task 1: Verify reference_score defaults to 0.0 for newly inserted symbols
#[test]
fn test_reference_score_defaults_to_zero() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Insert a file (foreign key requirement)
    db.store_file_info(&FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc123".to_string(),
        size: 100,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Insert a symbol via raw SQL (to avoid any future default-setting logic)
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('sym1', 'test_fn', 'function', 'rust', 'test.rs', 1, 10, 0, 1, 0, 100)",
            [],
        )
        .unwrap();

    // Verify reference_score defaults to 0.0
    let score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'sym1'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(
        (score - 0.0).abs() < f64::EPSILON,
        "reference_score should default to 0.0, got {}",
        score
    );
}
// ============================================================================
// Migration 011: Embedding Config (Phase 5, Task 1)
// ============================================================================

#[test]
fn test_migration_011_creates_embedding_config_table() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify embedding_config table exists
    let table_exists: bool = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='embedding_config'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )
        .unwrap();
    assert!(
        table_exists,
        "embedding_config table should exist after migration 011"
    );
}

#[test]
fn test_migration_011_is_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create database (runs all migrations including 011)
    {
        let _db = SymbolDatabase::new(&db_path).unwrap();
    }

    // Re-open — should not error
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();
    let version = db.get_schema_version().unwrap();
    assert_eq!(version, LATEST_SCHEMA_VERSION);

    // Config should still have defaults (including format_version from migration 014)
    let (model, dims, fmt_ver) = db.get_embedding_config().unwrap();
    assert_eq!(model, "bge-small-en-v1.5");
    assert_eq!(dims, 384);
    assert_eq!(fmt_ver, 1);
}

// ============================================================
// MIGRATION 027 — type_arguments table (Miller bridge Phase 2)
// ============================================================

fn table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

fn index_names(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'index' AND tbl_name = ?1 AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .unwrap();
    stmt.query_map([table], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

/// (cid-ordered) column shape: (name, declared_type, notnull, dflt_value, pk).
fn table_columns(
    conn: &rusqlite::Connection,
    table: &str,
) -> Vec<(String, String, i64, Option<String>, i64)> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(1)?,         // name
            row.get::<_, String>(2)?,         // type
            row.get::<_, i64>(3)?,            // notnull
            row.get::<_, Option<String>>(4)?, // dflt_value
            row.get::<_, i64>(5)?,            // pk
        ))
    })
    .unwrap()
    .collect::<rusqlite::Result<Vec<_>>>()
    .unwrap()
}

const TYPE_ARG_INDEXES: [&str; 4] = [
    "idx_type_args_file",
    "idx_type_args_identifier",
    "idx_type_args_name",
    "idx_type_args_parent",
];

/// Build a realistic v26-shaped database with populated parent rows so the
/// 26->27 migration can be exercised on a genuine upgrade (not a fresh DB).
///
/// Rather than hand-maintaining a brittle partial DDL copy (which omits columns
/// `initialize_schema`'s index creation depends on, e.g. `symbols.semantic_group`),
/// this builds a full current-schema DB via the real API, stores rows through
/// the canonical write path, then *downgrades* the on-disk shape to v26 by
/// dropping the migration-027 table and resetting the recorded version. The
/// parent tables keep their real columns; only the 027 delta is removed.
fn build_v26_database_with_rows(db_path: &std::path::Path) {
    {
        let mut db = SymbolDatabase::new(db_path).unwrap();
        let file = file_info_builder("legacy.cs").build();
        let symbol = symbol_builder("sym-legacy", "Legacy", "legacy.cs").build();
        let identifier = identifier_builder("id-legacy", "List", "legacy.cs").build();
        db.bulk_store_fresh_atomic(&[file], &[symbol], &[], &[identifier], &[], "primary")
            .unwrap();
    }

    let conn = open_test_connection(db_path).unwrap();
    conn.execute("DROP TABLE IF EXISTS type_arguments", [])
        .unwrap();
    conn.execute("DELETE FROM schema_version WHERE version >= 27", [])
        .unwrap();
}

#[test]
fn test_migration_fresh_database_has_type_arguments_table_and_indexes() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("fresh.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    assert!(
        table_exists(&db.conn, "type_arguments"),
        "fresh database at LATEST must have the type_arguments table"
    );
    let indexes = index_names(&db.conn, "type_arguments");
    for expected in TYPE_ARG_INDEXES {
        assert!(
            indexes.iter().any(|name| name == expected),
            "fresh database missing index {expected}; got {indexes:?}"
        );
    }
}

#[test]
fn test_migration_027_upgrades_v26_database_preserving_data() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("v26.db");
    build_v26_database_with_rows(&db_path);

    // Reopen with current code — should run migration 027 only.
    let db = SymbolDatabase::new(&db_path).unwrap();

    assert_eq!(
        db.get_schema_version().unwrap(),
        LATEST_SCHEMA_VERSION,
        "v26 database should migrate up to LATEST"
    );
    assert!(
        table_exists(&db.conn, "type_arguments"),
        "migration 027 must create the type_arguments table on upgrade"
    );
    let indexes = index_names(&db.conn, "type_arguments");
    for expected in TYPE_ARG_INDEXES {
        assert!(
            indexes.iter().any(|name| name == expected),
            "upgrade missing index {expected}; got {indexes:?}"
        );
    }

    // Pre-existing parent rows must survive the migration untouched.
    let file_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'legacy.cs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let symbol_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE id = 'sym-legacy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let identifier_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM identifiers WHERE id = 'id-legacy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(file_count, 1, "files row must survive migration");
    assert_eq!(symbol_count, 1, "symbols row must survive migration");
    assert_eq!(
        identifier_count, 1,
        "identifiers row must survive migration"
    );
}

#[test]
fn test_migration_027_is_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("idempotent.db");

    {
        let _db = SymbolDatabase::new(&db_path).unwrap();
    }
    // Re-open must not error and must hold the version + table steady.
    let db = SymbolDatabase::new(&db_path).unwrap();
    assert_eq!(db.get_schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    assert!(table_exists(&db.conn, "type_arguments"));
    assert_eq!(index_names(&db.conn, "type_arguments").len(), 4);
}

#[test]
fn test_type_arguments_schema_fresh_matches_migrated() {
    // Fresh-at-LATEST DB.
    let fresh_dir = TempDir::new().unwrap();
    let fresh_path = fresh_dir.path().join("fresh.db");
    let fresh_db = SymbolDatabase::new(&fresh_path).unwrap();

    // v26 DB migrated up to LATEST.
    let migrated_dir = TempDir::new().unwrap();
    let migrated_path = migrated_dir.path().join("migrated.db");
    build_v26_database_with_rows(&migrated_path);
    let migrated_db = SymbolDatabase::new(&migrated_path).unwrap();

    let fresh_cols = table_columns(&fresh_db.conn, "type_arguments");
    let migrated_cols = table_columns(&migrated_db.conn, "type_arguments");

    assert!(
        !fresh_cols.is_empty(),
        "type_arguments must have columns (guards against trivially-equal empty schemas)"
    );
    assert_eq!(
        fresh_cols, migrated_cols,
        "type_arguments column shape must be identical fresh vs migrated"
    );
    assert_eq!(
        index_names(&fresh_db.conn, "type_arguments"),
        index_names(&migrated_db.conn, "type_arguments"),
        "type_arguments index set must be identical fresh vs migrated"
    );
}

// ============================================================
// MIGRATION 028: literals table (Miller bridge Phase 3)
// ============================================================

const LITERAL_INDEXES: [&str; 3] = [
    "idx_literals_containing",
    "idx_literals_file",
    "idx_literals_kind",
];

/// Build a realistic v27-shaped database (type_arguments present, literals
/// absent) so the 27->28 migration can be exercised on a genuine upgrade.
/// Same downgrade-from-current strategy as `build_v26_database_with_rows`:
/// build a full current-schema DB via the real API, then drop only the
/// migration-028 delta and reset the recorded version to 27.
fn build_v27_database_with_rows(db_path: &std::path::Path) {
    {
        let mut db = SymbolDatabase::new(db_path).unwrap();
        let file = file_info_builder("legacy.cs").build();
        let symbol = symbol_builder("sym-legacy", "Legacy", "legacy.cs").build();
        let identifier = identifier_builder("id-legacy", "List", "legacy.cs").build();
        db.bulk_store_fresh_atomic(&[file], &[symbol], &[], &[identifier], &[], "primary")
            .unwrap();
    }

    let conn = open_test_connection(db_path).unwrap();
    conn.execute("DROP TABLE IF EXISTS literals", []).unwrap();
    conn.execute("DELETE FROM schema_version WHERE version >= 28", [])
        .unwrap();
}

#[test]
fn test_migration_fresh_database_has_literals_table_and_indexes() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("fresh.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    assert!(
        table_exists(&db.conn, "literals"),
        "fresh database at LATEST must have the literals table"
    );
    let indexes = index_names(&db.conn, "literals");
    for expected in LITERAL_INDEXES {
        assert!(
            indexes.iter().any(|name| name == expected),
            "fresh database missing index {expected}; got {indexes:?}"
        );
    }
}

#[test]
fn test_migration_028_upgrades_v27_database_preserving_data() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("v27.db");
    build_v27_database_with_rows(&db_path);

    // Reopen with current code — should run migration 028 only.
    let db = SymbolDatabase::new(&db_path).unwrap();

    assert_eq!(
        db.get_schema_version().unwrap(),
        LATEST_SCHEMA_VERSION,
        "v27 database should migrate up to LATEST"
    );
    assert!(
        table_exists(&db.conn, "literals"),
        "migration 028 must create the literals table on upgrade"
    );
    let indexes = index_names(&db.conn, "literals");
    for expected in LITERAL_INDEXES {
        assert!(
            indexes.iter().any(|name| name == expected),
            "upgrade missing index {expected}; got {indexes:?}"
        );
    }

    // Pre-existing parent rows must survive the migration untouched.
    let file_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'legacy.cs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let symbol_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE id = 'sym-legacy'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(file_count, 1, "files row must survive migration");
    assert_eq!(symbol_count, 1, "symbols row must survive migration");
    // type_arguments (the prior migration's table) must also survive.
    assert!(
        table_exists(&db.conn, "type_arguments"),
        "type_arguments must still exist after 028"
    );
}

#[test]
fn test_migration_028_is_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("idempotent.db");

    {
        let _db = SymbolDatabase::new(&db_path).unwrap();
    }
    // Re-open must not error and must hold the version + table steady.
    let db = SymbolDatabase::new(&db_path).unwrap();
    assert_eq!(db.get_schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    assert!(table_exists(&db.conn, "literals"));
    assert_eq!(index_names(&db.conn, "literals").len(), 3);
}

#[test]
fn test_literals_schema_fresh_matches_migrated() {
    // Fresh-at-LATEST DB.
    let fresh_dir = TempDir::new().unwrap();
    let fresh_path = fresh_dir.path().join("fresh.db");
    let fresh_db = SymbolDatabase::new(&fresh_path).unwrap();

    // v27 DB migrated up to LATEST.
    let migrated_dir = TempDir::new().unwrap();
    let migrated_path = migrated_dir.path().join("migrated.db");
    build_v27_database_with_rows(&migrated_path);
    let migrated_db = SymbolDatabase::new(&migrated_path).unwrap();

    let fresh_cols = table_columns(&fresh_db.conn, "literals");
    let migrated_cols = table_columns(&migrated_db.conn, "literals");

    assert!(
        !fresh_cols.is_empty(),
        "literals must have columns (guards against trivially-equal empty schemas)"
    );
    assert_eq!(
        fresh_cols, migrated_cols,
        "literals column shape must be identical fresh vs migrated"
    );
    assert_eq!(
        index_names(&fresh_db.conn, "literals"),
        index_names(&migrated_db.conn, "literals"),
        "literals index set must be identical fresh vs migrated"
    );
}

fn build_v28_database_with_rows(db_path: &std::path::Path) {
    {
        let mut db = SymbolDatabase::new(db_path).unwrap();
        let file = file_info_builder("legacy.rs").build();
        let symbol = symbol_builder("sym-legacy", "legacy", "legacy.rs").build();
        db.bulk_store_fresh_atomic(&[file], &[symbol], &[], &[], &[], "primary")
            .unwrap();
    }

    let conn = open_test_connection(db_path).unwrap();
    conn.execute("DROP TABLE IF EXISTS source_regions", [])
        .unwrap();
    conn.execute("DROP TABLE IF EXISTS structural_facts", [])
        .unwrap();
    conn.execute("DROP TABLE IF EXISTS complexity_metrics", [])
        .unwrap();
    conn.execute("DELETE FROM schema_version WHERE version >= 29", [])
        .unwrap();
}

#[test]
fn test_migration_029_adds_extractor_enrichment_tables() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("v28.db");
    build_v28_database_with_rows(&db_path);

    let db = SymbolDatabase::new(&db_path).unwrap();

    assert_eq!(db.get_schema_version().unwrap(), 29);
    for table in ["source_regions", "structural_facts", "complexity_metrics"] {
        assert!(
            table_exists(&db.conn, table),
            "migration 029 must create {table}"
        );
    }
}
