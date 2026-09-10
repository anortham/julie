use julie_extractors::ParseDiagnosticKind;

use super::{FakeExtractor, hash_of, upsert};
use crate::rows::{Normalization, StructuralFactQuery};
use crate::store::{FactsStore, Opened};
use crate::writer::FactsWriter;

fn populated() -> FactsStore {
    let mut store = FactsStore::in_memory().unwrap();
    let extractor = FakeExtractor::new();
    FactsWriter::new(&mut store, &extractor, Normalization::default())
        .apply(&[upsert("a.ts", "v1"), upsert("lib/b.ts", "v2")])
        .unwrap();
    store
}

#[test]
fn symbol_rows_join_path_and_compose_id() {
    let store = populated();
    let symbols = store.reader().symbols_for_paths(&["a.ts"]).unwrap();

    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[1].id, format!("{}:1", hash_of("v1")));
    assert_eq!(symbols[1].path, "a.ts");
    assert_eq!(symbols[1].language, "typescript");
    assert_eq!(symbols[1].name, "inner");
    assert_eq!(symbols[1].signature.as_deref(), Some("inner()"));
    assert_eq!(symbols[1].body_span.map(|b| b.start_line), Some(2));
    assert_eq!(symbols[1].span.start_line, 1);
    assert_eq!(symbols[1].confidence, Some(0.9));
}

#[test]
fn per_path_queries_return_only_that_path() {
    let store = populated();
    let reader = store.reader();

    let regions = reader.source_regions_for_path("a.ts").unwrap();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].kind, "comment");
    assert_eq!(regions[0].containing_ordinal, Some(0));

    let diagnostics = reader.diagnostics_for_path("lib/b.ts").unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].kind, ParseDiagnosticKind::Error);
    assert_eq!(diagnostics[0].message.as_deref(), Some("unexpected"));

    assert!(
        reader
            .diagnostics_for_path("missing.ts")
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        reader
            .symbols_for_paths(&["a.ts", "lib/b.ts"])
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn complexity_for_symbol_uses_the_composed_id() {
    let store = populated();
    let metrics = store
        .reader()
        .complexity_for_symbol(&format!("{}:1", hash_of("v1")))
        .unwrap();

    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].decision_count, 2);
    assert_eq!(metrics[0].symbol_ordinal, Some(1));
    assert!(
        store
            .reader()
            .complexity_for_symbol("nope")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn structural_facts_filter_by_pattern_path_and_limit() {
    let store = populated();
    let reader = store.reader();

    let all = reader
        .structural_facts(&StructuralFactQuery {
            pattern_ids: vec!["route".to_string()],
            ..StructuralFactQuery::default()
        })
        .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].capture_name, "handler");
    assert_eq!(all[0].containing_ordinal, Some(1));

    let under_lib = reader
        .structural_facts(&StructuralFactQuery {
            path_pattern: Some("lib/*".to_string()),
            ..StructuralFactQuery::default()
        })
        .unwrap();
    assert_eq!(under_lib.len(), 1);
    assert_eq!(under_lib[0].path, "lib/b.ts");

    let none = reader
        .structural_facts(&StructuralFactQuery {
            limit: 0,
            ..StructuralFactQuery::default()
        })
        .unwrap();
    assert!(none.is_empty());
}

#[test]
fn blob_count_and_file_size_report_the_store() {
    let store = populated();
    assert_eq!(store.reader().blob_count().unwrap(), 2);
    assert_eq!(store.reader().file_size_bytes().unwrap(), 0);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.sqlite");
    let Opened::Ready(store) = FactsStore::open(&path).unwrap() else {
        panic!("fresh file must open ready");
    };
    assert!(store.reader().file_size_bytes().unwrap() > 0);
}
