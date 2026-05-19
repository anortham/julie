use rusqlite::{Connection, params};
use tempfile::TempDir;

use crate::database::{LATEST_SCHEMA_VERSION, SymbolDatabase};
use crate::external_extract::{
    ExternalInfoSchemaState, ensure_external_extract_metadata,
    mark_external_extract_analysis_current, open_external_extract_database,
    read_external_extract_info,
};

fn schema_version(conn: &Connection) -> i32 {
    conn.query_row("SELECT MAX(version) FROM schema_version", [], |row| {
        row.get(0)
    })
    .expect("schema version exists")
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
        )",
        [table],
        |row| row.get::<_, bool>(0),
    )
    .expect("table existence query succeeds")
}

fn create_schema_version_only_db(db_path: &std::path::Path, version: i32) {
    let conn = Connection::open(db_path).expect("open sqlite db");
    conn.execute_batch(
        "CREATE TABLE schema_version (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL,
            description TEXT NOT NULL
        );",
    )
    .expect("create schema_version table");
    conn.execute(
        "INSERT INTO schema_version (version, applied_at, description)
         VALUES (?1, 1234567890, 'test schema')",
        params![version],
    )
    .expect("insert schema version");
}

#[test]
fn extract_info_is_read_only_and_does_not_migrate() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let old_version = LATEST_SCHEMA_VERSION - 1;
    create_schema_version_only_db(&db_path, old_version);

    let before = std::fs::metadata(&db_path)
        .expect("db metadata before")
        .modified()
        .expect("db mtime before");

    let info = read_external_extract_info(&db_path).expect("read external info");

    assert_eq!(info.schema_version, Some(old_version));
    assert_eq!(info.schema_state, ExternalInfoSchemaState::Older);
    assert!(info.metadata.is_none());
    assert!(info.missing_metadata_keys.len() >= 9);
    assert_eq!(info.counts.files, 0);
    assert_eq!(info.counts.symbols, 0);

    let conn = Connection::open(&db_path).expect("reopen db");
    assert_eq!(schema_version(&conn), old_version);
    assert!(!table_exists(&conn, "external_extract_metadata"));

    let after = std::fs::metadata(&db_path)
        .expect("db metadata after")
        .modified()
        .expect("db mtime after");
    assert_eq!(after, before, "info must not write to the database file");
}

#[test]
fn extract_metadata_generates_stable_workspace_id() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let root = temp_dir.path().join("repo");
    std::fs::create_dir(&root).expect("create root");

    let first_workspace_id = {
        let db = open_external_extract_database(&db_path, false).expect("open external db");
        let metadata = ensure_external_extract_metadata(&db, &root, None).expect("create metadata");
        uuid::Uuid::parse_str(&metadata.workspace_id).expect("generated workspace id is uuid");
        metadata.workspace_id
    };

    let second_workspace_id = {
        let db = open_external_extract_database(&db_path, false).expect("reopen external db");
        let metadata = ensure_external_extract_metadata(&db, &root, None).expect("reuse metadata");
        metadata.workspace_id
    };

    assert_eq!(second_workspace_id, first_workspace_id);

    let db = open_external_extract_database(&db_path, false).expect("reopen external db");
    ensure_external_extract_metadata(&db, &root, Some(&first_workspace_id))
        .expect("matching requested workspace id is accepted");
    let mismatch =
        ensure_external_extract_metadata(&db, &root, Some("00000000-0000-4000-8000-000000000000"))
            .expect_err("mismatched requested workspace id is rejected");
    assert!(
        mismatch.to_string().contains("workspace id mismatch"),
        "unexpected mismatch error: {mismatch}"
    );
}

#[test]
fn extract_strict_schema_rejects_older_db() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let old_version = LATEST_SCHEMA_VERSION - 1;
    create_schema_version_only_db(&db_path, old_version);

    let error = match open_external_extract_database(&db_path, true) {
        Ok(_) => panic!("strict schema should reject older db before migration"),
        Err(error) => error,
    };

    assert!(
        error.to_string().contains("older than current binary"),
        "unexpected strict schema error: {error}"
    );

    let conn = Connection::open(&db_path).expect("reopen db");
    assert_eq!(schema_version(&conn), old_version);
    assert!(!table_exists(&conn, "external_extract_metadata"));
}

#[test]
fn extract_info_rejects_newer_schema() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let future_version = LATEST_SCHEMA_VERSION + 1;
    create_schema_version_only_db(&db_path, future_version);

    let error = read_external_extract_info(&db_path)
        .expect_err("info should reject a db newer than the binary");

    assert!(
        error.to_string().contains("newer than current binary"),
        "unexpected newer schema error: {error}"
    );

    let conn = Connection::open(&db_path).expect("reopen db");
    assert_eq!(schema_version(&conn), future_version);
    assert!(!table_exists(&conn, "external_extract_metadata"));
}

#[test]
fn external_extract_metadata_table_created_by_migration() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");

    let db = SymbolDatabase::new(&db_path).expect("create db");
    assert_eq!(db.get_schema_version().unwrap(), LATEST_SCHEMA_VERSION);

    let conn = Connection::open(&db_path).expect("reopen db");
    assert!(table_exists(&conn, "external_extract_metadata"));
}

#[test]
fn extract_analysis_current_marker_rolls_back_on_partial_failure() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let root = temp_dir.path().join("repo");
    std::fs::create_dir(&root).expect("create root");

    let db = open_external_extract_database(&db_path, false).expect("open external db");
    ensure_external_extract_metadata(&db, &root, None).expect("create metadata");
    db.conn
        .execute(
            "UPDATE external_extract_metadata SET value = 'stale' WHERE key = 'analysis_state'",
            [],
        )
        .expect("mark stale setup");
    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_external_analyzed_revision
             BEFORE UPDATE OF value ON external_extract_metadata
             WHEN OLD.key = 'analyzed_revision'
             BEGIN
                SELECT RAISE(ABORT, 'forced analyzed revision failure');
             END;",
        )
        .expect("create failure trigger");

    let error = mark_external_extract_analysis_current(&db, Some(7))
        .expect_err("marker should fail and roll back");
    assert!(
        error
            .to_string()
            .contains("forced analyzed revision failure"),
        "unexpected marker error: {error}"
    );

    let analysis_state: String = db
        .conn
        .query_row(
            "SELECT value FROM external_extract_metadata WHERE key = 'analysis_state'",
            [],
            |row| row.get(0),
        )
        .expect("read analysis state");
    assert_eq!(analysis_state, "stale");
}
