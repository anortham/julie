use super::{FakeExtractor, hash_of, upsert};
use crate::rows::{EncoderRow, Normalization, VectorRow};
use crate::store::FactsStore;
use crate::writer::FactsWriter;

fn encoder(id: &str, checksum: &str) -> EncoderRow {
    EncoderRow {
        id: id.to_string(),
        model_checksum: checksum.to_string(),
        dimensions: 3,
        pooling: "cls".to_string(),
        normalization: "l2".to_string(),
        instruction_policy: "v1".to_string(),
    }
}

fn row(blob_hash: &str, ordinal: u32, vector: Vec<f32>) -> VectorRow {
    VectorRow {
        blob_hash: blob_hash.to_string(),
        symbol_ordinal: ordinal,
        vector,
    }
}

fn store_with_paths() -> FactsStore {
    let mut store = FactsStore::in_memory().unwrap();
    let extractor = FakeExtractor::new();
    FactsWriter::new(&mut store, &extractor, Normalization::default())
        .apply(&[upsert("a.ts", "alpha"), upsert("b.ts", "beta")])
        .unwrap();
    store
}

#[test]
fn store_vectors_round_trips_rows_for_the_encoder() {
    let mut store = store_with_paths();
    let extractor = FakeExtractor::new();
    let mut writer = FactsWriter::new(&mut store, &extractor, Normalization::default());
    writer.set_encoder(&encoder("enc-a", "aaaa")).unwrap();
    writer
        .store_vectors(
            "enc-a",
            &[
                row(&hash_of("alpha"), 0, vec![1.0, 0.0, 0.0]),
                row(&hash_of("beta"), 1, vec![0.0, 0.5, -0.25]),
            ],
        )
        .unwrap();

    let reader = store.reader();
    assert_eq!(reader.encoder().unwrap(), Some(encoder("enc-a", "aaaa")));
    let rows = reader.vectors_for_encoder("enc-a").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].blob_hash, hash_of("alpha"));
    assert_eq!(rows[0].symbol_ordinal, 0);
    assert_eq!(rows[0].vector, vec![1.0, 0.0, 0.0]);
    assert_eq!(rows[1].vector, vec![0.0, 0.5, -0.25]);
    assert_eq!(reader.vector_count().unwrap(), 2);
    assert!(reader.vectors_for_encoder("enc-b").unwrap().is_empty());
}

#[test]
fn vector_count_ignores_blobs_no_path_holds() {
    let mut store = store_with_paths();
    let extractor = FakeExtractor::new();
    let mut writer = FactsWriter::new(&mut store, &extractor, Normalization::default());
    writer.set_encoder(&encoder("enc-a", "aaaa")).unwrap();
    writer
        .store_vectors("enc-a", &[row(&hash_of("alpha"), 0, vec![1.0, 0.0, 0.0])])
        .unwrap();
    writer.apply(&[upsert("a.ts", "alpha-changed")]).unwrap();

    assert_eq!(store.reader().vector_count().unwrap(), 0);
    assert_eq!(
        store.reader().vectors_for_encoder("enc-a").unwrap().len(),
        1
    );
}

#[test]
fn set_encoder_with_a_different_identity_deletes_every_vector() {
    let mut store = store_with_paths();
    let extractor = FakeExtractor::new();
    let mut writer = FactsWriter::new(&mut store, &extractor, Normalization::default());
    writer.set_encoder(&encoder("enc-a", "aaaa")).unwrap();
    writer
        .store_vectors("enc-a", &[row(&hash_of("alpha"), 0, vec![1.0, 0.0, 0.0])])
        .unwrap();

    writer.set_encoder(&encoder("enc-b", "bbbb")).unwrap();

    let reader = store.reader();
    assert_eq!(reader.encoder().unwrap(), Some(encoder("enc-b", "bbbb")));
    assert!(reader.vectors_for_encoder("enc-a").unwrap().is_empty());
    assert_eq!(reader.vector_count().unwrap(), 0);
}

#[test]
fn set_encoder_with_the_same_identity_keeps_vectors() {
    let mut store = store_with_paths();
    let extractor = FakeExtractor::new();
    let mut writer = FactsWriter::new(&mut store, &extractor, Normalization::default());
    writer.set_encoder(&encoder("enc-a", "aaaa")).unwrap();
    writer
        .store_vectors("enc-a", &[row(&hash_of("alpha"), 0, vec![1.0, 0.0, 0.0])])
        .unwrap();

    writer.set_encoder(&encoder("enc-a", "aaaa")).unwrap();

    assert_eq!(store.reader().vector_count().unwrap(), 1);
}

#[test]
fn symbol_count_counts_symbols_of_held_paths() {
    let store = store_with_paths();
    assert_eq!(store.reader().symbol_count().unwrap(), 4);
}
