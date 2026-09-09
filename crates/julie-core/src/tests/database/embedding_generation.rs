use crate::database::SymbolDatabase;

#[test]
fn embedding_generation_is_unreadable_until_published() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();
    let generation = db.begin_embedding_generation("encoder-a", 12, 384).unwrap();
    assert!(!db.embedding_generation_ready("encoder-a", 12).unwrap());
    db.publish_embedding_generation(generation, 12, 0, 0)
        .unwrap();
    assert!(db.embedding_generation_ready("encoder-a", 12).unwrap());
    assert!(!db.embedding_generation_ready("encoder-b", 12).unwrap());
}

#[test]
fn test_late_batch_rejection_from_superseded_generation() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen1 = db.begin_embedding_generation("encoder-a", 10, 384).unwrap();
    let gen2 = db.begin_embedding_generation("encoder-a", 11, 384).unwrap();

    // gen1 is now superseded
    let pair = vec![("sym1".to_string(), vec![0.1_f32; 384])];
    let result = db.store_embeddings_for_generation(gen1, &pair);
    assert!(
        result.is_err(),
        "Late batch to superseded generation must be rejected"
    );

    // gen2 is active and accepts batches
    let stored = db.store_embeddings_for_generation(gen2, &pair).unwrap();
    assert_eq!(stored, 1);
}

#[test]
fn test_source_revision_advancing_during_embed() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let generation_id = db.begin_embedding_generation("encoder-a", 10, 384).unwrap();
    let pair = vec![("sym1".to_string(), vec![0.1_f32; 384])];
    db.store_embeddings_for_generation(generation_id, &pair)
        .unwrap();

    // Attempt to publish under mismatched revision 11 when started for 10
    let err = db.publish_embedding_generation(generation_id, 11, 1, 1);
    assert!(
        err.is_err(),
        "Publishing with mismatched revision must fail"
    );

    // Publishing under started revision 10 succeeds
    db.publish_embedding_generation(generation_id, 10, 1, 1)
        .unwrap();
    assert!(db.embedding_generation_ready("encoder-a", 10).unwrap());
    assert!(!db.embedding_generation_ready("encoder-a", 11).unwrap());
}

#[test]
fn test_same_dimensional_model_switch_invalidates_compatibility() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    // Both models produce 384-dimensional vectors
    let gen_a = db.begin_embedding_generation("encoder-a", 10, 384).unwrap();
    let pair_a = vec![("sym1".to_string(), vec![0.1_f32; 384])];
    db.store_embeddings_for_generation(gen_a, &pair_a).unwrap();
    db.publish_embedding_generation(gen_a, 10, 1, 1).unwrap();

    assert!(db.embedding_generation_ready("encoder-a", 10).unwrap());
    assert!(!db.embedding_generation_ready("encoder-b", 10).unwrap());
    assert!(db.get_embedding("sym1").unwrap().is_some());

    // Switch to encoder-b with identical dimensions
    let gen_b = db.begin_embedding_generation("encoder-b", 10, 384).unwrap();
    assert!(
        !db.embedding_generation_ready("encoder-a", 10).unwrap(),
        "Old encoder invalidated immediately"
    );
    assert!(
        !db.embedding_generation_ready("encoder-b", 10).unwrap(),
        "New encoder not yet published"
    );
    assert!(
        db.get_embedding("sym1").unwrap().is_none(),
        "Vectors cleared on model switch"
    );

    db.publish_embedding_generation(gen_b, 10, 0, 0).unwrap();
    assert!(!db.embedding_generation_ready("encoder-a", 10).unwrap());
    assert!(db.embedding_generation_ready("encoder-b", 10).unwrap());
}

#[test]
fn test_zero_eligible_symbol_workspace_is_valid_empty_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db.begin_embedding_generation("encoder-a", 1, 384).unwrap();
    db.publish_embedding_generation(gen_id, 1, 0, 0).unwrap();

    assert!(db.embedding_generation_ready("encoder-a", 1).unwrap());
    let results = db.knn_search(&[0.1_f32; 384], 5).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_mid_build_crash_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("symbols.db");
    {
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        let _gen = db.begin_embedding_generation("encoder-a", 10, 384).unwrap();
        // Simulate crash without publishing
    }

    // Reopen database
    let mut db = SymbolDatabase::new(&db_path).unwrap();
    assert!(!db.embedding_generation_ready("encoder-a", 10).unwrap());

    let cleaned = db.cleanup_stale_generations().unwrap();
    assert_eq!(cleaned, 1);

    // Starting a new generation succeeds cleanly
    let gen2 = db.begin_embedding_generation("encoder-a", 10, 384).unwrap();
    db.publish_embedding_generation(gen2, 10, 0, 0).unwrap();
    assert!(db.embedding_generation_ready("encoder-a", 10).unwrap());
}

#[test]
fn test_lexical_unaffected_during_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    // Insert a symbol
    crate::test_support::store_file_info_if_missing(
        &mut db,
        &crate::test_support::file_info_builder("src/main.rs")
            .language("rust")
            .hash("h1")
            .build(),
    )
    .unwrap();
    db.store_symbols(&[
        crate::test_support::symbol_builder("sym_main", "main", "src/main.rs").build(),
    ])
    .unwrap();

    // Start vector generation
    let _gen = db.begin_embedding_generation("encoder-a", 1, 384).unwrap();

    // Relational symbols table is completely unaffected and accessible
    let sym = db.get_symbol_by_id("sym_main").unwrap();
    assert!(sym.is_some());
    assert_eq!(sym.unwrap().name, "main");
}

#[test]
fn test_begin_embedding_generation_writes_format_version_3() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();
    let _gen = db
        .begin_embedding_generation("encoder-test", 1, 384)
        .unwrap();
    let (model, dims, fmt) = db.get_embedding_config().unwrap();
    assert_eq!(model, "encoder-test");
    assert_eq!(dims, 384);
    assert_eq!(fmt, crate::CURRENT_EMBEDDING_FORMAT_VERSION);
    assert_eq!(fmt, 3);
}
