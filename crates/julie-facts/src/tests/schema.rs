use std::collections::BTreeSet;

use crate::store::FactsStore;
use crate::version::{FACTS_SCHEMA_VERSION, SEMANTIC_INDEX_ENGINE_VERSION};

fn table_names(store: &FactsStore) -> BTreeSet<String> {
    let mut stmt = store
        .conn()
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
        .unwrap();
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn in_memory_creates_every_fact_table() {
    let store = FactsStore::in_memory().unwrap();
    let expected: BTreeSet<String> = [
        "meta",
        "blobs",
        "paths",
        "symbols",
        "identifiers",
        "relationships",
        "types",
        "source_regions",
        "structural_facts",
        "complexity_metrics",
        "literals",
        "type_arguments",
        "diagnostics",
        "encoder",
        "vectors",
        "test_verdicts",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    assert_eq!(table_names(&store), expected);
}

#[test]
fn in_memory_records_schema_and_engine_version() {
    let store = FactsStore::in_memory().unwrap();
    let schema: String = store
        .conn()
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let engine: String = store
        .conn()
        .query_row(
            "SELECT value FROM meta WHERE key = 'engine_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(schema, FACTS_SCHEMA_VERSION.to_string());
    assert_eq!(engine, SEMANTIC_INDEX_ENGINE_VERSION);
}

#[test]
fn writer_source_only_updates_or_deletes_paths() {
    let sources = [include_str!("../writer.rs"), include_str!("../insert.rs")];
    for source in sources {
        for keyword in ["UPDATE", "DELETE FROM"] {
            for (offset, _) in source.match_indices(keyword) {
                let rest = &source[offset + keyword.len()..];
                let target = rest.split_whitespace().next().unwrap_or("");
                assert_eq!(
                    target, "paths",
                    "{keyword} must target paths, found `{target}`"
                );
            }
        }
    }
}
