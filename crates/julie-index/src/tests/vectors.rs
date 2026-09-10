use std::path::{Path, PathBuf};
use std::time::Instant;

use julie_core::embeddings_contract::{SemanticMode, TaggedQueryEmbedding};
use julie_core::workspace::mutation_gate::{MutationGuard, Registry};
use julie_facts::rows::{EncoderRow, VectorRow};

use crate::checkout_store::{CheckoutStore, PathChange};
use crate::graph::SymbolId;
use crate::search::hybrid::hybrid_search_with_tagged_embedding;
use crate::search::index::SearchFilter;
use crate::search::similarity::{
    MIN_SIMILARITY_SCORE, QUERY_SIMILARITY_THRESHOLD, similar_to_query, similar_to_symbol,
};
use crate::snapshot::Snapshot;
use crate::vectors::VectorSet;

const DIMS: usize = 384;
const COUNT: usize = 10_000;

fn encoder() -> EncoderRow {
    EncoderRow {
        id: "enc-test".to_string(),
        model_checksum: "0".repeat(64),
        dimensions: DIMS as u32,
        pooling: "cls".to_string(),
        normalization: "l2".to_string(),
        instruction_policy: "v1".to_string(),
    }
}

fn encoder_of(dims: u32) -> EncoderRow {
    EncoderRow {
        dimensions: dims,
        ..encoder()
    }
}

fn row(ordinal: u32, vector: Vec<f32>) -> VectorRow {
    VectorRow {
        blob_hash: "blob".to_string(),
        symbol_ordinal: ordinal,
        vector,
    }
}

fn ordinal_of(key: &str) -> Option<SymbolId> {
    key.rsplit_once(':')
        .and_then(|(_, ordinal)| ordinal.parse().ok())
        .map(SymbolId)
}

fn random_vectors(count: usize, dims: usize, seed: u64) -> Vec<Vec<f32>> {
    let mut state = seed;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
    };
    (0..count)
        .map(|_| (0..dims).map(|_| next()).collect())
        .collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb)
}

#[test]
fn scan_top_five_matches_a_naive_reference_and_stays_under_budget() {
    let vectors = random_vectors(COUNT, DIMS, 7);
    let rows: Vec<VectorRow> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| row(i as u32, v.clone()))
        .collect();
    let set = VectorSet::from_rows(Some(encoder()), &rows, ordinal_of);
    let query = random_vectors(1, DIMS, 99).remove(0);

    let started = Instant::now();
    let top = set.scan(&query, 5);
    let elapsed = started.elapsed();

    let mut reference: Vec<(usize, f32)> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| (i, cosine(&query, v)))
        .collect();
    reference.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let expected: Vec<u32> = reference.iter().take(5).map(|(i, _)| *i as u32).collect();
    let got: Vec<u32> = top.iter().map(|(id, _)| id.0).collect();
    assert_eq!(got, expected);
    for ((_, score), (_, reference_score)) in top.iter().zip(reference.iter()) {
        assert!((score - reference_score).abs() < 1e-4);
    }
    eprintln!("scan of {COUNT} x {DIMS} took {elapsed:?}");
    assert!(
        elapsed.as_millis() < 500,
        "scan took {elapsed:?}, an order of magnitude over the 50 ms budget"
    );
}

#[test]
fn scan_returns_cosine_scores_sorted_descending() {
    let rows = vec![
        row(0, vec![0.0, 1.0]),
        row(1, vec![3.0, 4.0]),
        row(2, vec![2.0, 0.0]),
    ];
    let set = VectorSet::from_rows(Some(encoder_of(2)), &rows, ordinal_of);

    let top = set.scan(&[1.0, 0.0], 3);

    let ids: Vec<u32> = top.iter().map(|(id, _)| id.0).collect();
    assert_eq!(ids, vec![2, 1, 0]);
    assert!((top[0].1 - 1.0).abs() < 1e-6);
    assert!((top[1].1 - 0.6).abs() < 1e-6);
    assert!(top[2].1.abs() < 1e-6);
    assert_eq!(set.scan(&[1.0, 0.0], 1).len(), 1);
    assert_eq!(set.dims(), 2);
    assert_eq!(set.len(), 3);
}

#[test]
fn from_rows_skips_keys_the_resolver_does_not_know_and_wrong_dimensions() {
    let rows = vec![
        row(0, vec![1.0, 0.0]),
        row(1, vec![0.0, 1.0]),
        row(2, vec![1.0, 1.0, 1.0]),
        row(7, vec![1.0, 1.0]),
    ];
    let set = VectorSet::from_rows(Some(encoder_of(2)), &rows, |key| {
        ordinal_of(key).filter(|id| id.0 < 5)
    });

    assert_eq!(set.len(), 2);
    assert!(set.contains(SymbolId(0)));
    assert!(!set.contains(SymbolId(7)));
    assert!(set.vector_of(SymbolId(2)).is_none());
    assert!(set.scan(&[1.0, 1.0], 10).iter().all(|(id, _)| id.0 < 2));
}

#[test]
fn bind_maps_the_same_rows_onto_new_ids() {
    let rows = vec![row(0, vec![1.0, 0.0]), row(1, vec![0.0, 1.0])];
    let set = VectorSet::from_rows(Some(encoder_of(2)), &rows, ordinal_of);

    let rebound = set.bind(|key| ordinal_of(key).map(|id| SymbolId(id.0 + 10)));

    assert_eq!(rebound.len(), 2);
    assert_eq!(rebound.scan(&[1.0, 0.0], 1)[0].0, SymbolId(10));
    let dropped = set.bind(|_| None);
    assert!(dropped.is_empty());
    assert!(dropped.scan(&[1.0, 0.0], 1).is_empty());
}

#[test]
fn vector_of_returns_the_normalized_row() {
    let set = VectorSet::from_rows(Some(encoder_of(2)), &[row(0, vec![3.0, 4.0])], ordinal_of);

    let stored = set.vector_of(SymbolId(0)).unwrap();

    assert!((stored[0] - 0.6).abs() < 1e-6);
    assert!((stored[1] - 0.8).abs() < 1e-6);
    assert_eq!(set.encoder().map(|e| e.id.as_str()), Some("enc-test"));
}

#[test]
fn empty_set_scans_to_nothing() {
    let set = VectorSet::empty();
    assert!(set.is_empty());
    assert!(set.scan(&[1.0], 5).is_empty());
    assert!(set.encoder().is_none());
}

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
    Registry::new().try_acquire("vectors-test").unwrap()
}

fn seeded_store() -> (tempfile::TempDir, CheckoutStore) {
    let dir = tempfile::tempdir().unwrap();
    for name in ["app.py", "helpers.py", "config.py"] {
        std::fs::copy(seed_root().join(name), dir.path().join(name)).unwrap();
    }
    let store = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
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
    (dir, store)
}

fn unit(axis: usize) -> Vec<f32> {
    let mut v = vec![0.0; 4];
    v[axis] = 1.0;
    v
}

fn named_rows(snapshot: &Snapshot, vectors: &[(&str, Vec<f32>)]) -> Vec<VectorRow> {
    let graph = snapshot.graph();
    vectors
        .iter()
        .map(|(name, vector)| {
            let row = graph.symbol(graph.find_by_name(name)[0]);
            VectorRow {
                blob_hash: row.blob_hash.clone(),
                symbol_ordinal: row.ordinal,
                vector: vector.clone(),
            }
        })
        .collect()
}

fn small_encoder() -> EncoderRow {
    encoder_of(4)
}

fn id_of(snapshot: &Snapshot, name: &str) -> SymbolId {
    snapshot.graph().find_by_name(name)[0]
}

#[test]
fn store_vectors_then_publish_serves_a_bound_set_that_survives_apply() {
    let (dir, store) = seeded_store();
    let snapshot = store.current();
    assert!(snapshot.vectors().is_empty());
    let rows = named_rows(
        &snapshot,
        &[("alpha_app", unit(0)), ("load_config", unit(1))],
    );

    store.set_encoder(&small_encoder()).unwrap();
    store.store_vectors("enc-test", &rows).unwrap();
    assert!(store.current().vectors().is_empty());
    store.publish_vectors().unwrap();

    let published = store.current();
    assert_eq!(published.vectors().len(), 2);
    assert_eq!(store.status().vector_count, 2);
    let top = published.vectors().scan(&unit(0), 1);
    assert_eq!(top[0].0, id_of(&published, "alpha_app"));

    store
        .apply(
            &[PathChange::Remove {
                path: "config.py".to_string(),
            }],
            &guard(),
        )
        .unwrap();
    let after = store.current();
    assert_eq!(after.vectors().len(), 1);
    assert_eq!(store.status().vector_count, 1);
    assert_eq!(
        after.vectors().scan(&unit(1), 1)[0].0,
        id_of(&after, "alpha_app")
    );

    let reopened = CheckoutStore::open(&dir.path().join("index"), dir.path()).unwrap();
    assert_eq!(reopened.current().vectors().len(), 1);
}

#[test]
fn set_encoder_with_a_new_identity_publishes_an_empty_set() {
    let (_dir, store) = seeded_store();
    let rows = named_rows(&store.current(), &[("alpha_app", unit(0))]);
    store.set_encoder(&small_encoder()).unwrap();
    store.store_vectors("enc-test", &rows).unwrap();
    store.publish_vectors().unwrap();
    assert_eq!(store.current().vectors().len(), 1);

    store
        .set_encoder(&EncoderRow {
            id: "enc-other".to_string(),
            ..small_encoder()
        })
        .unwrap();

    assert!(store.current().vectors().is_empty());
    assert_eq!(
        store.current().vectors().encoder().map(|e| e.id.clone()),
        Some("enc-other".to_string())
    );
}

fn published_store() -> (tempfile::TempDir, CheckoutStore) {
    let (dir, store) = seeded_store();
    let rows = named_rows(
        &store.current(),
        &[
            ("alpha_app", vec![1.0, 0.0, 0.0, 0.0]),
            ("shared_helper_marker", vec![0.9, 0.1, 0.0, 0.0]),
            ("load_config", vec![0.0, 0.0, 1.0, 0.0]),
        ],
    );
    store.set_encoder(&small_encoder()).unwrap();
    store.store_vectors("enc-test", &rows).unwrap();
    store.publish_vectors().unwrap();
    (dir, store)
}

#[test]
fn similar_to_symbol_excludes_self_and_applies_the_threshold() {
    let (_dir, store) = published_store();
    let snapshot = store.current();

    let similar = similar_to_symbol(
        &snapshot,
        id_of(&snapshot, "alpha_app"),
        5,
        MIN_SIMILARITY_SCORE,
    );

    assert_eq!(similar.len(), 1);
    assert_eq!(similar[0].0, id_of(&snapshot, "shared_helper_marker"));
    assert!(similar[0].1 >= MIN_SIMILARITY_SCORE);
    assert!(
        similar_to_symbol(
            &snapshot,
            id_of(&snapshot, "load_config"),
            5,
            MIN_SIMILARITY_SCORE
        )
        .is_empty()
    );
    assert!(
        similar_to_symbol(
            &snapshot,
            id_of(&snapshot, "DEFAULT_TIMEOUT_SECONDS"),
            5,
            MIN_SIMILARITY_SCORE
        )
        .is_empty()
    );
}

#[test]
fn similar_to_query_applies_the_query_threshold() {
    let (_dir, store) = published_store();
    let snapshot = store.current();

    let similar = similar_to_query(
        &snapshot,
        &[0.0, 0.0, 1.0, 0.0],
        5,
        QUERY_SIMILARITY_THRESHOLD,
    );

    assert_eq!(similar.len(), 1);
    assert_eq!(similar[0].0, id_of(&snapshot, "load_config"));
}

fn tagged(vector: Vec<f32>, key: &str) -> TaggedQueryEmbedding {
    TaggedQueryEmbedding::new(vector, key, 0)
}

fn filter() -> SearchFilter {
    SearchFilter {
        language: None,
        kind: None,
        file_pattern: None,
        exclude_tests: false,
    }
}

#[test]
fn hybrid_search_merges_vector_hits_for_the_published_encoder() {
    let (_dir, store) = published_store();
    let snapshot = store.current();

    let results = hybrid_search_with_tagged_embedding(
        &snapshot,
        "zzz_no_lexical_match",
        &filter(),
        5,
        Some(tagged(vec![0.0, 0.0, 1.0, 0.0], "enc-test")),
        None,
        SemanticMode::Required,
    )
    .unwrap();

    assert_eq!(results.results[0].name, "load_config");
    assert_eq!(results.results[0].kind, "function");
    assert_eq!(results.results[0].file_path, "config.py");
    assert_eq!(
        results.results[0].id,
        snapshot.graph().symbol(id_of(&snapshot, "load_config")).id
    );
}

#[test]
fn hybrid_search_fails_closed_in_required_mode_when_the_encoder_differs() {
    let (_dir, store) = published_store();
    let snapshot = store.current();

    let err = match hybrid_search_with_tagged_embedding(
        &snapshot,
        "alpha",
        &filter(),
        5,
        Some(tagged(vec![0.0, 0.0, 1.0, 0.0], "enc-other")),
        None,
        SemanticMode::Required,
    ) {
        Ok(_) => panic!("required mode must fail closed"),
        Err(err) => err,
    };

    assert!(err.to_string().contains("SEMANTICS_NOT_READY"), "{err}");
    let degraded = hybrid_search_with_tagged_embedding(
        &snapshot,
        "alpha",
        &filter(),
        5,
        Some(tagged(vec![0.0, 0.0, 1.0, 0.0], "enc-other")),
        None,
        SemanticMode::Auto,
    )
    .unwrap();
    assert!(degraded.results.iter().all(|r| r.name != "load_config"));
}

#[test]
fn scan_records_its_last_duration() {
    let set = VectorSet::from_rows(None, &[], |_| None);
    assert_eq!(set.last_scan_micros(), None);
    let _ = set.scan(&[1.0, 0.0], 5);
    assert!(set.last_scan_micros().is_some());
}
