use std::sync::Arc;

use julie_extractors::SymbolKind;

use super::fixture::{edge_labels, edges_of_kind, file, graph_of, store_with};
use crate::graph::EdgeKind;

fn pair(from: &str, to: &str) -> (String, String) {
    (from.to_string(), to.to_string())
}

#[test]
fn apply_paths_shares_unchanged_file_rows_and_re_reads_changed_ones() {
    let (fixture, mut store) = store_with(vec![
        file("a.rs")
            .symbol("caller", "caller", SymbolKind::Function)
            .call("caller", "target"),
        file("b.rs").symbol("target", "target", SymbolKind::Function),
        file("c.rs")
            .symbol("other", "other", SymbolKind::Function)
            .call("other", "target"),
    ]);
    let before = graph_of(&store);

    fixture.put(
        &mut store,
        file("c.rs")
            .symbol("other", "other", SymbolKind::Function)
            .symbol("extra", "extra", SymbolKind::Function),
    );
    let after = before
        .apply_paths(&store.reader(), &["c.rs".to_string()], &[])
        .unwrap();

    for path in ["a.rs", "b.rs"] {
        let old = before.file_rows(path).unwrap();
        let new = after.file_rows(path).unwrap();
        assert!(
            Arc::ptr_eq(&old.symbols, &new.symbols),
            "{path} symbols re-read"
        );
        assert!(
            Arc::ptr_eq(&old.identifiers, &new.identifiers),
            "{path} identifiers re-read"
        );
        assert!(Arc::ptr_eq(&old.relationships, &new.relationships));
    }
    assert!(!Arc::ptr_eq(
        &before.file_rows("c.rs").unwrap().symbols,
        &after.file_rows("c.rs").unwrap().symbols
    ));
    assert_eq!(
        edges_of_kind(&before, EdgeKind::Calls),
        vec![
            pair("a.rs:caller", "b.rs:target"),
            pair("c.rs:other", "b.rs:target")
        ]
    );
    assert_eq!(
        edges_of_kind(&after, EdgeKind::Calls),
        vec![pair("a.rs:caller", "b.rs:target")]
    );
    assert_eq!(after.len(), 4);
}

#[test]
fn renaming_a_definition_re_resolves_callers_in_other_files() {
    let (fixture, mut store) = store_with(vec![
        file("a.rs")
            .symbol("caller", "caller", SymbolKind::Function)
            .call("caller", "target")
            .call("caller", "renamed"),
        file("b.rs").symbol("target", "target", SymbolKind::Function),
    ]);
    let before = graph_of(&store);
    assert_eq!(
        edges_of_kind(&before, EdgeKind::Calls),
        vec![pair("a.rs:caller", "b.rs:target")]
    );

    fixture.put(
        &mut store,
        file("b.rs").symbol("target", "renamed", SymbolKind::Function),
    );
    let after = before
        .apply_paths(&store.reader(), &["b.rs".to_string()], &[])
        .unwrap();

    assert_eq!(
        edges_of_kind(&after, EdgeKind::Calls),
        vec![pair("a.rs:caller", "b.rs:renamed")]
    );
    assert!(after.find_by_name("target").is_empty());
}

#[test]
fn removed_path_drops_its_symbols_and_edges() {
    let (fixture, mut store) = store_with(vec![
        file("a.rs")
            .symbol("caller", "caller", SymbolKind::Function)
            .call("caller", "target"),
        file("b.rs").symbol("target", "target", SymbolKind::Function),
    ]);
    let before = graph_of(&store);

    fixture.remove(&mut store, "b.rs");
    let after = before
        .apply_paths(&store.reader(), &[], &["b.rs".to_string()])
        .unwrap();

    assert_eq!(after.paths(), ["a.rs"]);
    assert_eq!(after.len(), 1);
    assert!(edge_labels(&after).is_empty());
    assert_eq!(before.len(), 2);
}
