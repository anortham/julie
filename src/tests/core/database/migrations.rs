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
