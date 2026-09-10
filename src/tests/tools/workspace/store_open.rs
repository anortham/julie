use std::fs;
use std::path::Path;

use julie_facts::version::{FACTS_SCHEMA_VERSION, SEMANTIC_INDEX_ENGINE_VERSION};
use rusqlite::Connection;
use tempfile::TempDir;

use crate::tools::workspace::indexing::store_open::open_or_recreate;

fn write_stale_facts(store_dir: &Path) {
    fs::create_dir_all(store_dir).unwrap();
    let db_path = store_dir.join("facts.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO meta (key, value) VALUES ('schema_version', '1');",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('engine_version', ?1)",
        [SEMANTIC_INDEX_ENGINE_VERSION],
    )
    .unwrap();
    assert_eq!(FACTS_SCHEMA_VERSION, 1);
    drop(conn);
    fs::write(store_dir.join("stale.marker"), "old").unwrap();
}

fn encoder_table_exists(store_dir: &Path) -> bool {
    let conn = Connection::open(store_dir.join("facts.sqlite")).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'encoder'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    count == 1
}

#[test]
fn failed_store_open_deletes_store_dir_and_reopens() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let store_dir = temp.path().join("store");
    write_stale_facts(&store_dir);

    let store = open_or_recreate(&store_dir, &root).expect("stale store must be recreated");
    drop(store);

    assert!(
        encoder_table_exists(&store_dir),
        "reopened store must have the encoder table"
    );
    assert!(
        !store_dir.join("stale.marker").exists(),
        "failed open must delete the old store directory"
    );
}

#[test]
fn version_mismatch_deletes_store_dir_and_reopens() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let store_dir = temp.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    let conn = Connection::open(store_dir.join("facts.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO meta (key, value) VALUES ('schema_version', '0'), ('engine_version', 'old');",
    )
    .unwrap();
    drop(conn);
    fs::write(store_dir.join("stale.marker"), "old").unwrap();

    open_or_recreate(&store_dir, &root).expect("mismatched store must be recreated");
    assert!(encoder_table_exists(&store_dir));
    assert!(!store_dir.join("stale.marker").exists());
}
