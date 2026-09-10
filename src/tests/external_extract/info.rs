use rusqlite::Connection;
use tempfile::TempDir;

use crate::external_extract::{
    EXTRACT_CONTRACT_VERSION, ExternalInfoSchemaState, ensure_external_extract_metadata,
    mark_external_extract_analysis_current, open_facts_store, read_external_extract_info,
};
use julie_facts::version::FACTS_SCHEMA_VERSION;

#[test]
fn extract_info_is_read_only_and_does_not_migrate() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let store = open_facts_store(&db_path, false).expect("create facts");
    drop(store);

    let before = std::fs::metadata(&db_path)
        .expect("db metadata before")
        .modified()
        .expect("db mtime before");

    let info = read_external_extract_info(&db_path).expect("read external info");
    assert_eq!(info.schema_version, Some(FACTS_SCHEMA_VERSION));
    assert_eq!(info.schema_state, ExternalInfoSchemaState::Current);
    assert_eq!(info.counts.files, 0);

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
        let store = open_facts_store(&db_path, false).expect("open facts");
        let metadata =
            ensure_external_extract_metadata(&store, &root, None).expect("create metadata");
        uuid::Uuid::parse_str(&metadata.workspace_id).expect("generated workspace id is uuid");
        let hash_algorithm: String = store
            .conn()
            .query_row(
                "SELECT value FROM meta WHERE key = 'hash_algorithm'",
                [],
                |row| row.get(0),
            )
            .expect("hash algorithm metadata exists");
        assert_eq!(hash_algorithm, "blake3");
        metadata.workspace_id
    };

    let second_workspace_id = {
        let store = open_facts_store(&db_path, false).expect("reopen facts");
        let metadata =
            ensure_external_extract_metadata(&store, &root, None).expect("reuse metadata");
        metadata.workspace_id
    };
    assert_eq!(second_workspace_id, first_workspace_id);

    let store = open_facts_store(&db_path, false).expect("reopen facts");
    ensure_external_extract_metadata(&store, &root, Some(&first_workspace_id))
        .expect("matching requested workspace id is accepted");
    let mismatch = ensure_external_extract_metadata(
        &store,
        &root,
        Some("00000000-0000-4000-8000-000000000000"),
    )
    .expect_err("mismatched requested workspace id is rejected");
    assert!(
        mismatch.to_string().contains("workspace id mismatch"),
        "unexpected mismatch error: {mismatch}"
    );
}

#[test]
fn extract_strict_schema_rejects_version_mismatch() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let store = open_facts_store(&db_path, false).expect("create facts");
    store
        .conn()
        .execute(
            "UPDATE meta SET value = '999' WHERE key = 'schema_version'",
            [],
        )
        .expect("bump schema");
    drop(store);

    let error = match open_facts_store(&db_path, true) {
        Ok(_) => panic!("strict schema should reject mismatch"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("version mismatch"),
        "unexpected strict schema error: {error}"
    );
}

#[test]
fn extract_info_rejects_newer_schema() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let store = open_facts_store(&db_path, false).expect("create facts");
    store
        .conn()
        .execute(
            "UPDATE meta SET value = '999' WHERE key = 'schema_version'",
            [],
        )
        .expect("bump schema");
    drop(store);

    let error = read_external_extract_info(&db_path)
        .expect_err("info should reject a db newer than the binary");
    assert!(
        error.to_string().contains("newer than current binary"),
        "unexpected newer schema error: {error}"
    );
}

#[test]
fn extract_analysis_current_marker_writes_meta() {
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("external.sqlite");
    let root = temp_dir.path().join("repo");
    std::fs::create_dir(&root).expect("create root");
    let store = open_facts_store(&db_path, false).expect("open facts");
    ensure_external_extract_metadata(&store, &root, None).expect("create metadata");
    mark_external_extract_analysis_current(&store, Some(7)).expect("mark current");
    let analysis_state: String = store
        .conn()
        .query_row(
            "SELECT value FROM meta WHERE key = 'analysis_state'",
            [],
            |row| row.get(0),
        )
        .expect("read analysis state");
    assert_eq!(analysis_state, "current");
}

#[test]
fn extract_contract_version_and_schema_are_at_phase3b_coordinated_values() {
    assert_eq!(
        EXTRACT_CONTRACT_VERSION, 3,
        "extract contract is v3 (workspace registry freshness / BLAKE3 hash metadata)"
    );
}

#[test]
fn facts_schema_version_is_one() {
    assert_eq!(FACTS_SCHEMA_VERSION, 1);
    let _ = Connection::open_in_memory();
}
