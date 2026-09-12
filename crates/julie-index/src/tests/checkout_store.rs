use std::path::{Path, PathBuf};
use std::sync::Arc;

use julie_core::workspace::mutation_gate::{MutationGuard, Registry};
use tantivy::Term;
use tantivy::collector::{Count, TopDocs};
use tantivy::query::{QueryParser, TermQuery};
use tantivy::schema::{IndexRecordOption, TantivyDocument, Value};

use crate::checkout_store::{CheckoutStore, PathChange, TantivyState, VersionMismatch};
use crate::snapshot::Snapshot;

fn seed_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/seed/a")
}

fn upsert(root: &Path, name: &str) -> PathChange {
    let bytes = std::fs::read(root.join(name)).unwrap();
    let language = julie_core::file_policy::detect_language_for_indexing_with_content(
        Path::new(name),
        &String::from_utf8_lossy(&bytes),
    );
    PathChange::Upsert {
        path: name.to_string(),
        bytes,
        language,
    }
}

fn guard() -> MutationGuard<'static> {
    Registry::new().try_acquire("checkout-store-test").unwrap()
}

fn copy_seed(into: &Path) {
    for name in ["app.py", "helpers.py", "config.py"] {
        std::fs::copy(seed_root().join(name), into.join(name)).unwrap();
    }
}

fn count(snapshot: &Snapshot, doc_type: &str) -> usize {
    let term = Term::from_field_text(snapshot.fields().doc_type, doc_type);
    snapshot
        .searcher()
        .search(&TermQuery::new(term, IndexRecordOption::Basic), &Count)
        .unwrap()
}

fn hits(snapshot: &Snapshot, query: &str) -> Vec<(String, String)> {
    let fields = snapshot.fields();
    let parser = QueryParser::for_index(snapshot.searcher().index(), vec![fields.name]);
    let query = parser.parse_query(query).unwrap();
    snapshot
        .searcher()
        .search(&query, &TopDocs::with_limit(10).order_by_score())
        .unwrap()
        .into_iter()
        .map(|(_, address)| {
            let doc: TantivyDocument = snapshot.searcher().doc(address).unwrap();
            let text = |field| {
                doc.get_first(field)
                    .and_then(|v| v.as_str())
                    .unwrap()
                    .to_string()
            };
            (text(fields.file_path), text(fields.name))
        })
        .collect()
}

fn hit(path: &str, name: &str) -> (String, String) {
    (path.to_string(), name.to_string())
}

fn symbol_names(snapshot: &Snapshot, path: &str) -> Vec<String> {
    let graph = snapshot.graph();
    graph
        .symbols_in_path(path)
        .iter()
        .map(|id| graph.symbol(*id).name.clone())
        .collect()
}

#[test]
fn apply_three_files_publishes_graph_symbols_and_searchable_names() {
    let dir = tempfile::tempdir().unwrap();
    copy_seed(dir.path());
    let store = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
    let guard = guard();

    let applied = store
        .apply(
            &[
                upsert(dir.path(), "app.py"),
                upsert(dir.path(), "helpers.py"),
                upsert(dir.path(), "config.py"),
            ],
            &guard,
        )
        .unwrap();
    assert_eq!(applied.new_blobs, 3);

    let snapshot = store.current();
    assert_eq!(
        snapshot.graph().paths(),
        ["app.py", "config.py", "helpers.py"]
    );
    assert!(symbol_names(&snapshot, "app.py").contains(&"alpha_app".to_string()));
    assert_eq!(
        symbol_names(&snapshot, "helpers.py")[0],
        "shared_helper_marker"
    );
    assert_eq!(count(&snapshot, "file"), 3);
    assert_eq!(count(&snapshot, "symbol"), snapshot.graph().len());
    assert_eq!(hits(&snapshot, "alpha_app"), [hit("app.py", "alpha_app")]);
    assert_eq!(
        snapshot.file_text("app.py").unwrap().unwrap(),
        std::fs::read_to_string(dir.path().join("app.py")).unwrap()
    );

    let status = store.status();
    assert_eq!(status.blob_count, 3);
    assert!(status.facts_bytes > 0);
    assert_eq!(status.tantivy, TantivyState::Present);
    assert_eq!(status.graph.symbols, snapshot.graph().len());
    assert!(status.last_write_at.is_some());
    assert!(dir.path().join("index/tantivy/julie.meta.json").is_file());
}

#[test]
#[ignore = "manual Windows diagnostic for parallel Tantivy segment creation"]
fn parallel_independent_stores_apply_without_segment_creation_failures() {
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let workers = (0..8)
        .map(|worker| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                for iteration in 0..10 {
                    let dir = tempfile::tempdir().unwrap();
                    copy_seed(dir.path());
                    let store = match CheckoutStore::open(&dir.path().join("index"), dir.path()) {
                        Ok(store) => store,
                        Err(error) => {
                            let preserved_root = dir.keep();
                            return Err(format!(
                                "worker={worker} iteration={iteration} open failed: {error:#}; preserved_root={}",
                                preserved_root.display()
                            ));
                        }
                    };
                    let changes = [
                        upsert(dir.path(), "app.py"),
                        upsert(dir.path(), "helpers.py"),
                        upsert(dir.path(), "config.py"),
                    ];
                    if let Err(error) = store.apply(&changes, &guard()) {
                        let preserved_root = dir.keep();
                        return Err(format!(
                            "worker={worker} iteration={iteration} apply failed: {error:#}; preserved_root={}",
                            preserved_root.display()
                        ));
                    }
                }
                Ok(())
            })
        })
        .collect::<Vec<_>>();

    let failures = workers
        .into_iter()
        .filter_map(|worker| match worker.join() {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(error) => Some(format!("worker panicked: {error:?}")),
        })
        .collect::<Vec<_>>();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn apply_change_keeps_the_old_snapshot_and_publishes_the_new_one() {
    let dir = tempfile::tempdir().unwrap();
    copy_seed(dir.path());
    let store = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
    let guard = guard();
    store
        .apply(&[upsert(dir.path(), "app.py")], &guard)
        .unwrap();
    let before = store.current();

    std::fs::write(
        dir.path().join("app.py"),
        "def renamed_app():\n    return 1\n",
    )
    .unwrap();
    store
        .apply(&[upsert(dir.path(), "app.py")], &guard)
        .unwrap();
    let after = store.current();

    assert!(!Arc::ptr_eq(&before, &after));
    assert!(symbol_names(&before, "app.py").contains(&"alpha_app".to_string()));
    assert_eq!(hits(&before, "alpha_app"), [hit("app.py", "alpha_app")]);
    assert_eq!(symbol_names(&after, "app.py"), ["renamed_app"]);
    assert_eq!(hits(&after, "renamed_app"), [hit("app.py", "renamed_app")]);
    assert!(hits(&after, "alpha_app").is_empty());
    assert_eq!(count(&after, "file"), 1);
    assert!(after.published_at() >= before.published_at());
}

#[test]
fn remove_drops_the_path_from_graph_and_index() {
    let dir = tempfile::tempdir().unwrap();
    copy_seed(dir.path());
    let store = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
    let guard = guard();
    store
        .apply(
            &[
                upsert(dir.path(), "app.py"),
                upsert(dir.path(), "helpers.py"),
            ],
            &guard,
        )
        .unwrap();

    let applied = store
        .apply(
            &[PathChange::Remove {
                path: "helpers.py".to_string(),
            }],
            &guard,
        )
        .unwrap();

    assert_eq!(applied.removed_paths, 1);
    let snapshot = store.current();
    assert_eq!(snapshot.graph().paths(), ["app.py"]);
    assert_eq!(count(&snapshot, "file"), 1);
    assert_eq!(
        hits(&snapshot, "shared_helper_marker"),
        [hit("app.py", "shared_helper_marker")]
    );
    assert!(snapshot.file_text("helpers.py").unwrap().is_none());
}

#[test]
fn reopen_without_tantivy_rebuilds_documents_from_facts() {
    let dir = tempfile::tempdir().unwrap();
    copy_seed(dir.path());
    let index_dir = dir.path().join("index");
    let expected = {
        let store = CheckoutStore::open(&index_dir, dir.path()).unwrap();
        store
            .apply(
                &[
                    upsert(dir.path(), "app.py"),
                    upsert(dir.path(), "helpers.py"),
                    upsert(dir.path(), "config.py"),
                ],
                &guard(),
            )
            .unwrap();
        let snapshot = store.current();
        (count(&snapshot, "file"), count(&snapshot, "symbol"))
    };
    std::fs::remove_dir_all(index_dir.join("tantivy")).unwrap();

    let store = CheckoutStore::open(&index_dir, dir.path()).unwrap();
    assert_eq!(store.status().tantivy, TantivyState::Absent);
    assert_eq!(store.current().graph().paths().len(), 3);
    assert!(store.rebuild_tantivy_if_needed(&guard()).unwrap());

    let snapshot = store.current();
    assert_eq!(store.status().tantivy, TantivyState::Present);
    assert_eq!(
        (count(&snapshot, "file"), count(&snapshot, "symbol")),
        expected
    );
    let mut found = hits(&snapshot, "shared_helper_marker");
    found.sort();
    assert_eq!(
        found,
        [
            hit("app.py", "shared_helper_marker"),
            hit("helpers.py", "shared_helper_marker")
        ]
    );
    assert!(!store.rebuild_tantivy_if_needed(&guard()).unwrap());
}

#[test]
fn file_text_returns_none_when_the_checkout_bytes_no_longer_match_facts() {
    let dir = tempfile::tempdir().unwrap();
    copy_seed(dir.path());
    let store = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
    store
        .apply(&[upsert(dir.path(), "app.py")], &guard())
        .unwrap();
    std::fs::write(dir.path().join("app.py"), "def edited(): pass\n").unwrap();

    assert!(store.current().file_text("app.py").unwrap().is_none());
    assert!(store.current().file_text("missing.py").unwrap().is_none());
}

#[test]
fn open_reports_a_facts_version_mismatch_instead_of_migrating() {
    let dir = tempfile::tempdir().unwrap();
    let index_dir = dir.path().join("index");
    drop(CheckoutStore::open(&index_dir, dir.path()).unwrap());
    let conn = rusqlite::Connection::open(index_dir.join("facts.sqlite")).unwrap();
    conn.execute(
        "UPDATE meta SET value = 'other' WHERE key = 'engine_version'",
        [],
    )
    .unwrap();
    drop(conn);

    let err = CheckoutStore::open(&index_dir, dir.path()).err().unwrap();
    let mismatch = err.downcast_ref::<VersionMismatch>().unwrap();
    assert_eq!(mismatch.found_engine, "other");
}
