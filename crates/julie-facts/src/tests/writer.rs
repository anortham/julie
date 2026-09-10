use std::collections::{HashMap, HashSet};

use julie_extractors::SymbolKind;

use super::{FakeExtractor, hash_of, upsert};
use crate::rows::{Normalization, TestRoleConfig};
use crate::store::{FactsStore, Opened};
use crate::writer::{FactsWriter, PathChange};

fn count(store: &FactsStore, table: &str) -> i64 {
    store
        .conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn same_bytes_under_two_paths_extracts_once() {
    let mut store = FactsStore::in_memory().unwrap();
    let extractor = FakeExtractor::new();
    let mut writer = FactsWriter::new(&mut store, &extractor, Normalization::default());

    let applied = writer
        .apply(&[upsert("a.ts", "same"), upsert("b.ts", "same")])
        .unwrap();

    assert_eq!(applied.new_blobs, 1);
    assert_eq!(applied.reused_blobs, 1);
    assert_eq!(extractor.calls.get(), 1);
    assert_eq!(
        applied.paths_now,
        vec![
            ("a.ts".to_string(), hash_of("same")),
            ("b.ts".to_string(), hash_of("same"))
        ]
    );
    assert_eq!(store.reader().blob_count().unwrap(), 1);
}

#[test]
fn changed_bytes_keep_old_rows_and_repoint_the_path() {
    let mut store = FactsStore::in_memory().unwrap();
    let extractor = FakeExtractor::new();
    let mut writer = FactsWriter::new(&mut store, &extractor, Normalization::default());
    writer.apply(&[upsert("a.ts", "v1")]).unwrap();

    let applied = writer.apply(&[upsert("a.ts", "v2")]).unwrap();

    assert_eq!(applied.new_blobs, 1);
    assert_eq!(count(&store, "symbols"), 4);
    assert_eq!(count(&store, "blobs"), 2);
    let paths = store.reader().paths().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].blob_hash, hash_of("v2"));
    let symbols = store.reader().symbols_for_paths(&["a.ts"]).unwrap();
    assert!(symbols.iter().all(|s| s.blob_hash == hash_of("v2")));
}

#[test]
fn remove_deletes_only_the_path_row() {
    let mut store = FactsStore::in_memory().unwrap();
    let extractor = FakeExtractor::new();
    let mut writer = FactsWriter::new(&mut store, &extractor, Normalization::default());
    writer.apply(&[upsert("a.ts", "v1")]).unwrap();

    let applied = writer
        .apply(&[PathChange::Remove {
            path: "a.ts".to_string(),
        }])
        .unwrap();

    assert_eq!(applied.removed_paths, 1);
    assert!(applied.paths_now.is_empty());
    assert!(store.reader().paths().unwrap().is_empty());
    assert_eq!(count(&store, "blobs"), 1);
    assert_eq!(count(&store, "symbols"), 2);
}

#[test]
fn ordinals_map_ids_within_the_blob() {
    let mut store = FactsStore::in_memory().unwrap();
    let extractor = FakeExtractor::new();
    FactsWriter::new(&mut store, &extractor, Normalization::default())
        .apply(&[upsert("a.ts", "v1")])
        .unwrap();
    let hash = hash_of("v1");
    let reader = store.reader();

    let symbols = reader.symbols_for_paths(&["a.ts"]).unwrap();
    assert_eq!(symbols[0].ordinal, 0);
    assert_eq!(symbols[0].parent_ordinal, None);
    assert_eq!(symbols[1].ordinal, 1);
    assert_eq!(symbols[1].parent_ordinal, Some(0));
    assert_eq!(symbols[1].kind, SymbolKind::Method);

    let identifiers = reader.identifiers_for_paths(&["a.ts"]).unwrap();
    assert_eq!(identifiers[0].containing_ordinal, Some(1));
    assert_eq!(identifiers[0].receiver_type.as_deref(), Some("Outer"));

    let relationships = reader.relationships_for_paths(&["a.ts"]).unwrap();
    assert_eq!(relationships.len(), 2);
    assert_eq!(relationships[0].from_ordinal, Some(1));
    assert_eq!(relationships[0].to_name, "Outer");
    assert_eq!(relationships[0].to_ordinal, Some(0));
    assert_eq!(
        relationships[0].to_blob_hash.as_deref(),
        Some(hash.as_str())
    );
    assert_eq!(relationships[1].to_name, "remote_fn");
    assert_eq!(relationships[1].to_ordinal, None);
    assert_eq!(relationships[1].to_blob_hash, None);

    let types = reader.types_for_paths(&["a.ts"]).unwrap();
    assert_eq!(types[0].symbol_ordinal, 1);
    assert_eq!(types[0].generic_params, Some(vec!["T".to_string()]));
}

#[test]
fn literals_keep_only_recognized_carriers_and_flatten_type_arguments() {
    let mut store = FactsStore::in_memory().unwrap();
    let extractor = FakeExtractor::new();
    FactsWriter::new(&mut store, &extractor, Normalization::default())
        .apply(&[upsert("a.ts", "v1")])
        .unwrap();

    let kinds: Vec<String> = {
        let mut stmt = store
            .conn()
            .prepare("SELECT kind FROM literals ORDER BY ordinal")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(kinds, vec!["url".to_string()]);

    let args: Vec<(u32, u32, Option<u32>, String)> = {
        let mut stmt = store
            .conn()
            .prepare(
                "SELECT ordinal, identifier_ordinal, parent_ordinal, type_name FROM type_arguments ORDER BY ordinal",
            )
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(
        args,
        vec![
            (0, 0, None, "List".to_string()),
            (1, 0, Some(0), "int".to_string())
        ]
    );
}

#[test]
fn test_role_config_marks_annotated_callables() {
    let mut store = FactsStore::in_memory().unwrap();
    let extractor = FakeExtractor::new();
    let mut roles = HashMap::new();
    roles.insert(
        "typescript".to_string(),
        TestRoleConfig {
            test_case: HashSet::from(["test".to_string()]),
            ..TestRoleConfig::default()
        },
    );
    let normalization = Normalization { test_roles: roles };
    FactsWriter::new(&mut store, &extractor, normalization)
        .apply(&[upsert("a.ts", "v1")])
        .unwrap();

    let symbols = store.reader().symbols_for_paths(&["a.ts"]).unwrap();
    let inner = &symbols[1];
    let metadata = inner.metadata.as_ref().unwrap();
    assert_eq!(metadata["test_role"], serde_json::json!("test_case"));
    assert_eq!(metadata["is_test"], serde_json::json!(true));
    assert!(symbols[0].metadata.is_none());
}

#[test]
fn ordinals_are_stable_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.sqlite");
    let extractor = FakeExtractor::new();
    let first = {
        let Opened::Ready(mut store) = FactsStore::open(&path).unwrap() else {
            panic!("fresh file must open ready");
        };
        FactsWriter::new(&mut store, &extractor, Normalization::default())
            .apply(&[upsert("a.ts", "v1")])
            .unwrap();
        rows(&store)
    };

    let Opened::Ready(store) = FactsStore::open(&path).unwrap() else {
        panic!("matching file must open ready");
    };

    assert_eq!(rows(&store), first);
    assert_eq!(first[0].0, format!("{}:0", hash_of("v1")));
}

fn rows(store: &FactsStore) -> Vec<(String, String, u32, Option<u32>)> {
    store
        .reader()
        .symbols_for_paths(&["a.ts"])
        .unwrap()
        .into_iter()
        .map(|s| (s.id, s.name, s.ordinal, s.parent_ordinal))
        .collect()
}
