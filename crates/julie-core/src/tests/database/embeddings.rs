use super::*;

#[test]
fn test_delete_embeddings_for_symbol_ids_only_removes_requested_rows() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert one file and three symbols for embedding rows.
    db.store_file_info(
        &file_info_builder("src/lib.rs")
            .language("rust")
            .hash("hash123")
            .size(100)
            .last_modified(0)
            .last_indexed(0)
            .symbol_count(0)
            .line_count(0)
            .build(),
    )
    .unwrap();

    let symbols: Vec<_> = [("sym_a", "a"), ("sym_b", "b"), ("sym_c", "c")]
        .iter()
        .map(|(id, name)| {
            symbol_builder(*id, *name, "src/lib.rs")
                .kind(SymbolKind::Function)
                .language("rust")
                .span(1, 0, 1, 1)
                .bytes(0, 1)
                .confidence(1.0)
                .build()
        })
        .collect();
    db.store_symbols(&symbols).unwrap();

    db.store_embeddings(&[
        ("sym_a".to_string(), vec![0.1_f32; 384]),
        ("sym_b".to_string(), vec![0.2_f32; 384]),
        ("sym_c".to_string(), vec![0.3_f32; 384]),
    ])
    .unwrap();

    let empty_deleted = db.delete_embeddings_for_symbol_ids(&[]).unwrap();
    assert_eq!(empty_deleted, 0, "empty input should delete nothing");
    assert_eq!(db.embedding_count().unwrap(), 3);

    let selected_ids = vec!["sym_a".to_string(), "sym_c".to_string()];
    let deleted = db.delete_embeddings_for_symbol_ids(&selected_ids).unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(db.embedding_count().unwrap(), 1);

    let remaining = db.get_embedded_symbol_ids().unwrap();
    assert!(remaining.contains("sym_b"));
    assert!(!remaining.contains("sym_a"));
    assert!(!remaining.contains("sym_c"));
}

#[test]
fn test_delete_embeddings_for_symbol_ids_batches_large_inputs() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let existing_ids: Vec<String> = (0..10).map(|i| format!("present_{i}")).collect();
    let embeddings: Vec<(String, Vec<f32>)> = existing_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), vec![i as f32; 384]))
        .collect();
    db.store_embeddings(&embeddings).unwrap();
    assert_eq!(db.embedding_count().unwrap(), 10);

    let mut delete_ids: Vec<String> = (0..40_000).map(|i| format!("missing_{i}")).collect();
    delete_ids.extend(existing_ids.iter().cloned());

    let deleted = db.delete_embeddings_for_symbol_ids(&delete_ids).unwrap();
    assert_eq!(deleted, existing_ids.len());
    assert_eq!(db.embedding_count().unwrap(), 0);

    let remaining_ids = db.get_embedded_symbol_ids().unwrap();
    assert!(remaining_ids.is_empty());
}
