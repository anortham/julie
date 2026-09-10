use crate::store::{FactsStore, Opened};
use crate::version::{FACTS_SCHEMA_VERSION, SEMANTIC_INDEX_ENGINE_VERSION};

const CRATE_MANIFEST: &str = include_str!("../../Cargo.toml");

fn pinned_extractors_tag() -> &'static str {
    CRATE_MANIFEST
        .lines()
        .find(|line| line.starts_with("julie-extractors") && line.contains("tag ="))
        .and_then(|line| line.split("tag =").nth(1))
        .and_then(|rest| rest.split('"').nth(1))
        .expect("Cargo.toml must pin julie-extractors to a git tag")
}

#[test]
fn engine_version_composes_contract_tag_and_facts_schema() {
    let expected = format!(
        "extractors={}+extractors-tag={}+facts={}",
        julie_extractors::EXTRACTION_CONTRACT_VERSION,
        pinned_extractors_tag(),
        FACTS_SCHEMA_VERSION
    );
    assert_eq!(SEMANTIC_INDEX_ENGINE_VERSION, expected);
}

fn file_hash(path: &std::path::Path) -> String {
    blake3::hash(&std::fs::read(path).unwrap())
        .to_hex()
        .to_string()
}

#[test]
fn open_reopens_a_matching_store_as_ready() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.sqlite");
    let Opened::Ready(store) = FactsStore::open(&path).unwrap() else {
        panic!("fresh file must open ready");
    };
    drop(store);
    assert!(matches!(FactsStore::open(&path).unwrap(), Opened::Ready(_)));
}

#[test]
fn open_returns_version_mismatch_without_touching_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("facts.sqlite");
    drop(FactsStore::open(&path).unwrap());
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "UPDATE meta SET value = 'other-engine' WHERE key = 'engine_version'",
            [],
        )
        .unwrap();
    }
    let before = file_hash(&path);

    let opened = FactsStore::open(&path).unwrap();

    match opened {
        Opened::VersionMismatch {
            found_schema,
            found_engine,
        } => {
            assert_eq!(found_schema, FACTS_SCHEMA_VERSION);
            assert_eq!(found_engine, "other-engine");
        }
        Opened::Ready(_) => panic!("mismatched engine version must not open ready"),
    }
    assert_eq!(file_hash(&path), before);
}
