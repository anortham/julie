use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use julie_index::checkout_store::{CheckoutStore, PathChange};
use rusqlite::Connection;
use tempfile::TempDir;

use tracing_subscriber::layer::SubscriberExt;

use crate::dashboard::error_buffer::ErrorBuffer;
use crate::handler::JulieServerHandler;
use crate::registry::database::DaemonDatabase;
use crate::request_engine::runtime_factory::{initialize_recovering_store, warn_on_repair_failure};
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::store_open::{open_or_recreate, store_for_workspace};
use crate::workspace::open_or_recreate_store;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

const PROBE_FILE: &str = "lib.rs";
const PROBE_SOURCE: &str = "pub fn probe_one() {}\npub fn probe_two() {}\n";

async fn build_store_with_one_rust_file(store_dir: &Path, root: &Path) {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join(PROBE_FILE), PROBE_SOURCE).unwrap();
    let store = CheckoutStore::open(store_dir, root).unwrap();
    let guard = julie_core::workspace::mutation_gate::acquire_gate("store_open_probe").await;
    store
        .apply(
            &[PathChange::Upsert {
                path: PROBE_FILE.to_string(),
                bytes: PROBE_SOURCE.as_bytes().to_vec(),
                language: "rust".to_string(),
            }],
            &guard,
        )
        .unwrap();
}

fn facts_path(store_dir: &Path) -> PathBuf {
    store_dir.join("facts.sqlite")
}

fn symbol_count(store_dir: &Path) -> i64 {
    let conn = Connection::open(facts_path(store_dir)).unwrap();
    conn.query_row("SELECT count(*) FROM symbols", [], |row| row.get(0))
        .unwrap()
}

fn hold_exclusive_lock(store_dir: &Path) -> Connection {
    let conn = Connection::open(facts_path(store_dir)).unwrap();
    let mode: String = conn
        .query_row("PRAGMA locking_mode = EXCLUSIVE", [], |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "exclusive");
    conn.query_row("SELECT count(*) FROM meta", [], |row| row.get::<_, i64>(0))
        .unwrap();
    conn
}

#[tokio::test]
async fn locked_store_is_not_deleted_on_open_failure() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    let store_dir = temp.path().join("store");
    build_store_with_one_rust_file(&store_dir, &root).await;
    let before = symbol_count(&store_dir);
    assert!(before > 0);

    let lock = hold_exclusive_lock(&store_dir);

    let opened = {
        let store_dir = store_dir.clone();
        let root = root.clone();
        tokio::task::spawn_blocking(move || open_or_recreate(&store_dir, &root))
            .await
            .unwrap()
    };

    assert!(opened.is_err(), "a locked store must not open");
    assert!(facts_path(&store_dir).exists());

    drop(lock);
    assert_eq!(symbol_count(&store_dir), before);
}

#[tokio::test]
async fn version_mismatch_store_is_deleted_and_rebuilt() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    let store_dir = temp.path().join("store");
    build_store_with_one_rust_file(&store_dir, &root).await;
    assert!(symbol_count(&store_dir) > 0);

    {
        let conn = Connection::open(facts_path(&store_dir)).unwrap();
        conn.execute(
            "UPDATE meta SET value = 'stale' WHERE key = 'engine_version'",
            [],
        )
        .unwrap();
    }

    let stale_marker = store_dir.join("stale.marker");
    fs::write(&stale_marker, b"stale").unwrap();

    let store = open_or_recreate(&store_dir, &root).expect("a mismatched store must be rebuilt");
    drop(store);
    assert!(!stale_marker.exists());
    assert_eq!(symbol_count(&store_dir), 0);
}

#[tokio::test]
async fn initialize_recovering_store_returns_error_without_deleting() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let workspace_id = generate_workspace_id(&root.to_string_lossy()).unwrap();
    let index_root = temp.path().join("indexes").join(&workspace_id);
    build_store_with_one_rust_file(&index_root, &root).await;
    let before = symbol_count(&index_root);
    assert!(before > 0);

    let handler = JulieServerHandler::new_in_process(
        WorkspaceStartupHint {
            path: root.clone(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        None,
        Some(index_root.clone()),
    )
    .await
    .unwrap();

    let lock = hold_exclusive_lock(&index_root);
    let outcome = initialize_recovering_store(&handler, &index_root).await;

    assert!(outcome.is_err(), "a locked store must not initialize");
    assert!(facts_path(&index_root).exists());

    drop(lock);
    assert_eq!(symbol_count(&index_root), before);
}

#[tokio::test]
async fn version_mismatch_on_a_secondary_checkout_is_rebuilt() {
    let temp = TempDir::new().unwrap();
    let primary_root = temp.path().join("primary");
    let secondary_root = temp.path().join("secondary");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&secondary_root).unwrap();
    let primary_root = primary_root.canonicalize().unwrap();
    let secondary_root = secondary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_root.to_string_lossy()).unwrap();
    let secondary_id = generate_workspace_id(&secondary_root.to_string_lossy()).unwrap();
    let indexes = temp.path().join("indexes");
    let secondary_store_dir = indexes.join(&secondary_id);
    build_store_with_one_rust_file(&secondary_store_dir, &secondary_root).await;
    assert!(symbol_count(&secondary_store_dir) > 0);

    {
        let conn = Connection::open(facts_path(&secondary_store_dir)).unwrap();
        conn.execute(
            "UPDATE meta SET value = 'stale' WHERE key = 'engine_version'",
            [],
        )
        .unwrap();
    }

    let handler = JulieServerHandler::new_in_process(
        WorkspaceStartupHint {
            path: primary_root.clone(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        None,
        Some(indexes.join(&primary_id)),
    )
    .await
    .unwrap();

    let stale_marker = secondary_store_dir.join("stale.marker");
    fs::write(&stale_marker, b"stale").unwrap();

    let store = store_for_workspace(&handler, &secondary_id, &secondary_root)
        .await
        .expect("a mismatched secondary store must be rebuilt");
    drop(store);
    assert!(!stale_marker.exists());
    assert_eq!(symbol_count(&secondary_store_dir), 0);
}

fn open_error_text<T>(result: anyhow::Result<T>) -> String {
    match result {
        Ok(_) => panic!("a corrupt store must not open"),
        Err(err) => format!("{err:#}"),
    }
}

#[test]
fn open_failure_error_names_the_store_dir() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    let store_dir = temp.path().join("store");
    fs::create_dir_all(&store_dir).unwrap();
    fs::write(facts_path(&store_dir), b"this is not a sqlite database").unwrap();

    let expected = store_dir.display().to_string();

    let checkout_err = open_error_text(open_or_recreate(&store_dir, &root));
    assert!(checkout_err.contains(&expected), "{checkout_err}");

    let runtime_err = open_error_text(open_or_recreate_store(&store_dir, &root));
    assert!(runtime_err.contains(&expected), "{runtime_err}");
}

#[test]
fn dropped_repair_failure_is_logged_with_the_index_root() {
    let buffer = ErrorBuffer::new(8);
    let subscriber = tracing_subscriber::registry().with(buffer.layer());
    let index_root = Path::new("/probe/indexes/ws_probe");

    tracing::subscriber::with_default(subscriber, || {
        warn_on_repair_failure(index_root, Ok(None));
        warn_on_repair_failure(index_root, Err(anyhow::anyhow!("facts.sqlite is locked")));
    });

    let entries = buffer.recent_entries();
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].level, "WARN");
    assert!(
        entries[0].message.contains("/probe/indexes/ws_probe"),
        "{}",
        entries[0].message
    );
    assert!(
        entries[0].message.contains("facts.sqlite is locked"),
        "{}",
        entries[0].message
    );
}

#[tokio::test]
async fn a_rebuilt_checkout_store_is_not_cached_empty() {
    let temp = TempDir::new().unwrap();
    let primary_root = temp.path().join("primary");
    let secondary_root = temp.path().join("secondary");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&secondary_root).unwrap();
    let primary_root = primary_root.canonicalize().unwrap();
    let secondary_root = secondary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_root.to_string_lossy()).unwrap();
    let secondary_id = generate_workspace_id(&secondary_root.to_string_lossy()).unwrap();
    let indexes = temp.path().join("indexes");
    let secondary_store_dir = indexes.join(&secondary_id);
    build_store_with_one_rust_file(&secondary_store_dir, &secondary_root).await;

    {
        let conn = Connection::open(facts_path(&secondary_store_dir)).unwrap();
        conn.execute(
            "UPDATE meta SET value = 'stale' WHERE key = 'engine_version'",
            [],
        )
        .unwrap();
    }

    let handler = JulieServerHandler::new_in_process(
        WorkspaceStartupHint {
            path: primary_root.clone(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        None,
        Some(indexes.join(&primary_id)),
    )
    .await
    .unwrap();

    let rebuilt = handler
        .checkout_store_for_workspace(&secondary_id, &secondary_root)
        .await
        .expect("a mismatched secondary store must be rebuilt");
    assert_eq!(rebuilt.status().graph.symbols, 0);
    drop(rebuilt);

    build_store_with_one_rust_file(&secondary_store_dir, &secondary_root).await;

    let reopened = handler
        .checkout_store_for_workspace(&secondary_id, &secondary_root)
        .await
        .expect("the reindexed secondary store must open");
    assert!(
        reopened.status().graph.symbols > 0,
        "a rebuilt store must not be cached, or the checkout stays empty for the session"
    );
}

#[tokio::test]
async fn status_over_a_locked_checkout_reports_not_loaded_and_deletes_nothing() {
    let temp = TempDir::new().unwrap();
    let primary_root = temp.path().join("primary");
    let locked_root = temp.path().join("locked");
    fs::create_dir_all(&primary_root).unwrap();
    fs::create_dir_all(&locked_root).unwrap();
    let primary_root = primary_root.canonicalize().unwrap();
    let locked_root = locked_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_root.to_string_lossy()).unwrap();
    let locked_path = locked_root.to_string_lossy().to_string();
    let locked_id = generate_workspace_id(&locked_path).unwrap();
    let indexes = temp.path().join("indexes");
    let locked_store_dir = indexes.join(&locked_id);
    build_store_with_one_rust_file(&locked_store_dir, &locked_root).await;
    let before = symbol_count(&locked_store_dir);
    assert!(before > 0);

    let daemon_db = Arc::new(DaemonDatabase::open(&temp.path().join("daemon.db")).unwrap());
    daemon_db
        .upsert_workspace(&locked_id, &locked_path, "ready")
        .unwrap();

    let handler = JulieServerHandler::new_in_process_with_daemon_db(
        WorkspaceStartupHint {
            path: primary_root.clone(),
            source: Some(WorkspaceStartupSource::Cli),
        },
        None,
        Some(indexes.join(&primary_id)),
        Some(Arc::clone(&daemon_db)),
    )
    .await
    .unwrap();

    let lock = hold_exclusive_lock(&locked_store_dir);

    let result = ManageWorkspaceTool {
        operation: "status".to_string(),
        path: None,
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .expect("status must report a locked checkout instead of failing");

    let structured = result
        .structured_content
        .expect("status must return structured content");
    let checkouts = structured["checkouts"]
        .as_array()
        .expect("status must list checkouts")
        .clone();
    let row = checkouts
        .iter()
        .find(|row| row["workspace_id"] == locked_id.as_str())
        .unwrap_or_else(|| panic!("status must include the locked checkout: {checkouts:?}"));
    assert_eq!(row["tantivy"], "absent", "{row:?}");
    assert_eq!(row["symbol_count"], 0, "{row:?}");
    assert_eq!(row["blob_count"], 0, "{row:?}");
    assert_eq!(row["last_write_at"], serde_json::Value::Null, "{row:?}");

    assert!(facts_path(&locked_store_dir).exists());
    drop(lock);
    assert_eq!(symbol_count(&locked_store_dir), before);
}
