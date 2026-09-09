use crate::database::{EmbeddingGeneration, EmbeddingGenerationStatus, SymbolDatabase};

/// Challenge 1: In-progress / building generation must be strictly unreadable by
/// `embedding_generation_ready` and `get_latest_ready_generation`.
#[test]
fn challenge_in_progress_building_generation_strictly_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen1 = db.begin_embedding_generation("encoder-a", 10, 384).unwrap();

    // In-progress: status is Building
    let meta = db.get_embedding_generation(gen1).unwrap().unwrap();
    assert_eq!(meta.status, EmbeddingGenerationStatus::Building);
    assert!(!meta.is_ready());

    // embedding_generation_ready must return false
    assert!(
        !db.embedding_generation_ready("encoder-a", 10).unwrap(),
        "Building generation must not be ready for matching rev"
    );
    assert!(
        !db.embedding_generation_ready("encoder-a", 0).unwrap(),
        "Building generation must not be ready for rev 0"
    );
    assert!(
        !db.embedding_generation_ready("encoder-b", 10).unwrap(),
        "Building generation must not be ready for wrong encoder"
    );

    // get_latest_ready_generation must return None
    assert!(
        db.get_latest_ready_generation().unwrap().is_none(),
        "No ready generation should exist while building"
    );

    // Storing a batch does not make it ready
    let pair = vec![("sym1".to_string(), vec![0.1_f32; 384])];
    db.store_embeddings_for_generation(gen1, &pair).unwrap();
    assert!(
        !db.embedding_generation_ready("encoder-a", 10).unwrap(),
        "Generation must remain unreadable after partial vector batch write"
    );
    assert!(db.get_latest_ready_generation().unwrap().is_none());

    // Only after explicit publish does it become ready
    db.publish_embedding_generation(gen1, 10, 1, 1).unwrap();
    assert!(
        db.embedding_generation_ready("encoder-a", 10).unwrap(),
        "Must be ready once published"
    );
    let ready_meta = db.get_latest_ready_generation().unwrap().unwrap();
    assert_eq!(ready_meta.id, gen1);
    assert_eq!(ready_meta.status, EmbeddingGenerationStatus::Ready);

    // As soon as a subsequent generation starts, the previous ready generation is superseded
    let gen2 = db.begin_embedding_generation("encoder-a", 10, 384).unwrap();
    assert!(
        !db.embedding_generation_ready("encoder-a", 10).unwrap(),
        "Previous ready generation must be invalidated immediately when new build starts"
    );
    assert!(
        db.get_latest_ready_generation().unwrap().is_none(),
        "get_latest_ready_generation must return None while gen2 is building"
    );

    let old_meta = db.get_embedding_generation(gen1).unwrap().unwrap();
    assert_eq!(old_meta.status, EmbeddingGenerationStatus::Superseded);
    let new_meta = db.get_embedding_generation(gen2).unwrap().unwrap();
    assert_eq!(new_meta.status, EmbeddingGenerationStatus::Building);
}

/// Challenge 2: Switching from encoder-a (384-dim) to encoder-b (384-dim) must
/// strictly purge old vectors and invalidate compatibility atomically.
#[test]
fn challenge_same_dim_encoder_switch_strictly_purges_and_invalidates() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    // Phase 1: Build and publish under encoder-a
    let gen_a = db
        .begin_embedding_generation("bge-small-en-v1.5-f32", 5, 384)
        .unwrap();
    let pairs_a = vec![
        ("sym_alpha".to_string(), vec![0.5_f32; 384]),
        ("sym_beta".to_string(), vec![0.8_f32; 384]),
    ];
    db.store_embeddings_for_generation(gen_a, &pairs_a).unwrap();
    db.publish_embedding_generation(gen_a, 5, 2, 2).unwrap();

    // Verify encoder-a is ready and vectors are readable
    assert!(
        db.embedding_generation_ready("bge-small-en-v1.5-f32", 5)
            .unwrap()
    );
    assert!(db.get_embedding("sym_alpha").unwrap().is_some());
    assert!(db.get_embedding("sym_beta").unwrap().is_some());
    let initial_knn = db.knn_search(&[0.5_f32; 384], 5).unwrap();
    assert_eq!(initial_knn.len(), 2);

    // Phase 2: Begin generation with encoder-b (same 384 dimensions)
    let gen_b = db
        .begin_embedding_generation("all-minilm-l6-v2-f32", 5, 384)
        .unwrap();

    // Invariants during building:
    // 1. encoder-a must be strictly invalidated (superseded)
    assert!(
        !db.embedding_generation_ready("bge-small-en-v1.5-f32", 5)
            .unwrap(),
        "encoder-a must be invalidated upon encoder-b begin"
    );
    // 2. encoder-b is not yet ready
    assert!(
        !db.embedding_generation_ready("all-minilm-l6-v2-f32", 5)
            .unwrap(),
        "encoder-b must not be ready while building"
    );
    // 3. get_latest_ready_generation returns None
    assert!(db.get_latest_ready_generation().unwrap().is_none());
    // 4. Physical vectors from encoder-a must be strictly purged
    assert!(
        db.get_embedding("sym_alpha").unwrap().is_none(),
        "sym_alpha vector must be purged on model switch"
    );
    assert!(
        db.get_embedding("sym_beta").unwrap().is_none(),
        "sym_beta vector must be purged on model switch"
    );
    assert!(
        db.knn_search(&[0.5_f32; 384], 5).unwrap().is_empty(),
        "KNN search must return empty once vectors are purged"
    );
    // 5. Late writes to gen_a must be refused
    let late_batch = vec![("sym_alpha".to_string(), vec![0.5_f32; 384])];
    assert!(
        db.store_embeddings_for_generation(gen_a, &late_batch)
            .is_err(),
        "Late batch to superseded gen_a must be refused"
    );

    // Phase 3: Store and publish under encoder-b
    let pairs_b = vec![("sym_gamma".to_string(), vec![0.3_f32; 384])];
    db.store_embeddings_for_generation(gen_b, &pairs_b).unwrap();
    db.publish_embedding_generation(gen_b, 5, 1, 1).unwrap();

    // Verify encoder-b is ready, encoder-a remains false, old vectors are gone, new vector present
    assert!(
        !db.embedding_generation_ready("bge-small-en-v1.5-f32", 5)
            .unwrap()
    );
    assert!(
        db.embedding_generation_ready("all-minilm-l6-v2-f32", 5)
            .unwrap()
    );
    assert!(db.get_embedding("sym_alpha").unwrap().is_none());
    assert!(db.get_embedding("sym_gamma").unwrap().is_some());
    let knn_b = db.knn_search(&[0.3_f32; 384], 5).unwrap();
    assert_eq!(knn_b.len(), 1);
    assert_eq!(knn_b[0].0, "sym_gamma");
}

/// Challenge 3: Publishing with a mismatched source revision must fail and leave
/// the generation un-published in Building status.
#[test]
fn challenge_publish_mismatched_source_revision_fails() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db
        .begin_embedding_generation("encoder-a", 100, 384)
        .unwrap();
    let pair = vec![("sym_test".to_string(), vec![0.1_f32; 384])];
    db.store_embeddings_for_generation(gen_id, &pair).unwrap();

    // Publish with older revision 99 -> must fail
    let err_stale = db.publish_embedding_generation(gen_id, 99, 1, 1);
    assert!(
        err_stale.is_err(),
        "Publishing with stale revision 99 must fail"
    );
    let err_msg = err_stale.unwrap_err().to_string();
    assert!(
        err_msg.contains("source revision mismatch"),
        "Error message should mention revision mismatch: {}",
        err_msg
    );

    // Publish with advanced revision 101 -> must fail
    let err_adv = db.publish_embedding_generation(gen_id, 101, 1, 1);
    assert!(
        err_adv.is_err(),
        "Publishing with advanced revision 101 must fail"
    );

    // Publish with rev 0 -> must fail
    let err_zero = db.publish_embedding_generation(gen_id, 0, 1, 1);
    assert!(err_zero.is_err(), "Publishing with rev 0 must fail");

    // Verify status is still Building (failed publish calls must not alter status)
    let meta = db.get_embedding_generation(gen_id).unwrap().unwrap();
    assert_eq!(meta.status, EmbeddingGenerationStatus::Building);
    assert!(!db.embedding_generation_ready("encoder-a", 100).unwrap());

    // Publishing with exact matching revision 100 succeeds
    db.publish_embedding_generation(gen_id, 100, 1, 1).unwrap();
    assert!(db.embedding_generation_ready("encoder-a", 100).unwrap());
    assert!(!db.embedding_generation_ready("encoder-a", 99).unwrap());
    assert!(!db.embedding_generation_ready("encoder-a", 101).unwrap());
}

/// Challenge 4: Vector batches cannot be stored into superseded, failed, ready,
/// or non-existent generations.
#[test]
fn challenge_store_embeddings_refuses_superseded_failed_ready_or_invalid_generation() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();
    let batch = vec![("sym_x".to_string(), vec![0.1_f32; 384])];

    // Case 4a: Storing into a Ready (already published) generation fails
    let gen_ready = db.begin_embedding_generation("encoder-a", 1, 384).unwrap();
    db.publish_embedding_generation(gen_ready, 1, 0, 0).unwrap();
    let err_ready = db.store_embeddings_for_generation(gen_ready, &batch);
    assert!(
        err_ready.is_err(),
        "Storing into ready generation must be refused"
    );
    assert!(
        err_ready
            .unwrap_err()
            .to_string()
            .contains("not in building state")
    );

    // Case 4b: Storing into a Superseded generation fails
    let gen_super1 = db.begin_embedding_generation("encoder-a", 2, 384).unwrap();
    let _gen_super2 = db.begin_embedding_generation("encoder-a", 3, 384).unwrap();
    let err_super = db.store_embeddings_for_generation(gen_super1, &batch);
    assert!(
        err_super.is_err(),
        "Storing into superseded generation must be refused"
    );
    assert!(
        err_super
            .unwrap_err()
            .to_string()
            .contains("not in building state")
    );

    // Case 4c: Storing into a Failed generation fails
    let gen_fail = db.begin_embedding_generation("encoder-a", 4, 384).unwrap();
    db.fail_embedding_generation(gen_fail).unwrap();
    let err_fail = db.store_embeddings_for_generation(gen_fail, &batch);
    assert!(
        err_fail.is_err(),
        "Storing into failed generation must be refused"
    );
    assert!(
        err_fail
            .unwrap_err()
            .to_string()
            .contains("not in building state")
    );

    // Case 4d: Storing into non-existent generation IDs fails
    let err_nonexistent = db.store_embeddings_for_generation(99999, &batch);
    assert!(
        err_nonexistent.is_err(),
        "Storing into non-existent ID 99999 must be refused"
    );
    let err_negative = db.store_embeddings_for_generation(-1, &batch);
    assert!(
        err_negative.is_err(),
        "Storing into negative ID -1 must be refused"
    );
}

/// Challenge 5: Zero-eligible-symbol project publishes cleanly as a valid empty
/// index without failing or panicking.
#[test]
fn challenge_zero_eligible_symbol_workspace_publishes_clean_empty_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db
        .begin_embedding_generation("encoder-zero", 1, 384)
        .unwrap();
    db.publish_embedding_generation(gen_id, 1, 0, 0).unwrap();

    assert!(
        db.embedding_generation_ready("encoder-zero", 1).unwrap(),
        "Empty workspace generation must be ready"
    );

    let generation = db.get_latest_ready_generation().unwrap().unwrap();
    assert_eq!(generation.eligible_symbols, 0);
    assert_eq!(generation.embedded_symbols, 0);
    assert!(generation.is_ready());
    assert!(generation.is_complete());
    assert!((generation.coverage_ratio() - 1.0).abs() < f64::EPSILON);

    // KNN query on valid empty index must return empty results, not fail or panic
    let knn = db.knn_search(&[0.1_f32; 384], 10).unwrap();
    assert!(
        knn.is_empty(),
        "KNN on empty index must return empty vector"
    );
}

/// Challenge 6: Process termination mid-build leaves generation strictly unreadable;
/// restart recovery cleans up stale generations safely without affecting ready generations.
#[test]
fn challenge_mid_build_termination_and_crash_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("symbols.db");

    // Session 1: Start building, write partial embeddings, simulate abrupt process crash
    {
        let mut db = SymbolDatabase::new(&db_path).unwrap();
        let gen_id = db
            .begin_embedding_generation("encoder-crash", 42, 384)
            .unwrap();
        let pair = vec![("sym_crash_1".to_string(), vec![0.7_f32; 384])];
        db.store_embeddings_for_generation(gen_id, &pair).unwrap();
        // Drop db abruptly without publish
    }

    // Session 2: Fresh startup after crash
    {
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        // 1. Generation must remain unreadable
        assert!(
            !db.embedding_generation_ready("encoder-crash", 42).unwrap(),
            "Mid-build crash generation must not be ready upon restart"
        );
        assert!(
            db.get_latest_ready_generation().unwrap().is_none(),
            "No ready generation after crash"
        );

        // 2. Generation status in DB is still 'building'
        let gen1 = db.get_embedding_generation(1).unwrap().unwrap();
        assert_eq!(gen1.status, EmbeddingGenerationStatus::Building);

        // 3. Recovery cleanup transitions leftover 'building' to 'failed'
        let cleaned = db.cleanup_stale_generations().unwrap();
        assert_eq!(cleaned, 1, "Should clean up exactly 1 stale generation");

        let gen1_after = db.get_embedding_generation(1).unwrap().unwrap();
        assert_eq!(gen1_after.status, EmbeddingGenerationStatus::Failed);

        // 4. Still unreadable after cleanup
        assert!(!db.embedding_generation_ready("encoder-crash", 42).unwrap());

        // 5. Subsequent build succeeds and publishes cleanly
        let gen2 = db
            .begin_embedding_generation("encoder-crash", 42, 384)
            .unwrap();
        let complete_batch = vec![
            ("sym_crash_1".to_string(), vec![0.7_f32; 384]),
            ("sym_crash_2".to_string(), vec![0.9_f32; 384]),
        ];
        db.store_embeddings_for_generation(gen2, &complete_batch)
            .unwrap();
        db.publish_embedding_generation(gen2, 42, 2, 2).unwrap();

        assert!(db.embedding_generation_ready("encoder-crash", 42).unwrap());
        assert_eq!(db.knn_search(&[0.7_f32; 384], 5).unwrap().len(), 2);
    }
}

/// Challenge 7: State machine invalid transition rejection.
/// Only generations in 'building' status may transition to 'ready' or 'failed'.
#[test]
fn challenge_state_machine_invalid_transitions() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    // 7a: Cannot publish already-published (Ready) generation (double-publish)
    let gen1 = db.begin_embedding_generation("encoder-a", 1, 384).unwrap();
    db.publish_embedding_generation(gen1, 1, 0, 0).unwrap();
    let err_double = db.publish_embedding_generation(gen1, 1, 0, 0);
    assert!(
        err_double.is_err(),
        "Double publishing a generation must fail"
    );
    assert!(
        err_double
            .unwrap_err()
            .to_string()
            .contains("expected 'building'")
    );

    // 7b: Cannot publish a Failed generation
    let gen2 = db.begin_embedding_generation("encoder-a", 2, 384).unwrap();
    db.fail_embedding_generation(gen2).unwrap();
    let err_failed = db.publish_embedding_generation(gen2, 2, 0, 0);
    assert!(
        err_failed.is_err(),
        "Publishing a failed generation must fail"
    );
    assert!(
        err_failed
            .unwrap_err()
            .to_string()
            .contains("expected 'building'")
    );

    // 7c: Cannot publish a Superseded generation
    let gen3a = db.begin_embedding_generation("encoder-a", 3, 384).unwrap();
    let _gen3b = db.begin_embedding_generation("encoder-a", 3, 384).unwrap();
    let err_super = db.publish_embedding_generation(gen3a, 3, 0, 0);
    assert!(
        err_super.is_err(),
        "Publishing a superseded generation must fail"
    );
    assert!(
        err_super
            .unwrap_err()
            .to_string()
            .contains("expected 'building'")
    );

    // 7d: Cannot publish non-existent generation ID
    let err_missing = db.publish_embedding_generation(99999, 1, 0, 0);
    assert!(
        err_missing.is_err(),
        "Publishing non-existent generation must fail"
    );
    assert!(err_missing.unwrap_err().to_string().contains("not found"));
}

/// Challenge 8: Dimension switch (384 -> 512) recreates vectors table,
/// invalidating old vectors and allowing new dimension embeddings.
#[test]
fn challenge_dimension_change_recreates_table_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    // 1. Build with 384 dimensions (bge-small)
    let gen384 = db
        .begin_embedding_generation("bge-small-en-v1.5-f32", 1, 384)
        .unwrap();
    db.store_embeddings_for_generation(gen384, &[("sym_384".to_string(), vec![0.1_f32; 384])])
        .unwrap();
    db.publish_embedding_generation(gen384, 1, 1, 1).unwrap();

    assert!(
        db.embedding_generation_ready("bge-small-en-v1.5-f32", 1)
            .unwrap()
    );
    assert!(db.get_embedding("sym_384").unwrap().is_some());

    // 2. Switch to 512 dimensions (qwen3-0.6b)
    let gen512 = db
        .begin_embedding_generation("qwen3-0.6b-f16", 1, 512)
        .unwrap();

    // Invariants:
    assert!(
        !db.embedding_generation_ready("bge-small-en-v1.5-f32", 1)
            .unwrap()
    );
    assert!(!db.embedding_generation_ready("qwen3-0.6b-f16", 1).unwrap());
    // Old 384-dim vector is wiped by table recreation
    assert!(db.get_embedding("sym_384").unwrap().is_none());

    // Storing 512-dim vectors succeeds
    db.store_embeddings_for_generation(gen512, &[("sym_512".to_string(), vec![0.2_f32; 512])])
        .unwrap();
    db.publish_embedding_generation(gen512, 1, 1, 1).unwrap();

    assert!(db.embedding_generation_ready("qwen3-0.6b-f16", 1).unwrap());
    assert!(db.get_embedding("sym_512").unwrap().is_some());

    // KNN with 512-dim query succeeds
    let knn = db.knn_search(&[0.2_f32; 512], 5).unwrap();
    assert_eq!(knn.len(), 1);
    assert_eq!(knn[0].0, "sym_512");
}

/// Challenge 9: `cleanup_stale_generations` strictly preserves existing `Ready` generations.
#[test]
fn challenge_cleanup_stale_generations_preserves_published_ready_generations() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen1 = db.begin_embedding_generation("encoder-a", 10, 384).unwrap();
    let embeddings: Vec<(String, Vec<f32>)> = (0..5)
        .map(|i| (format!("sym_{i}"), vec![0.1_f32; 384]))
        .collect();
    db.store_embeddings_for_generation(gen1, &embeddings)
        .unwrap();
    db.publish_embedding_generation(gen1, 10, 5, 5).unwrap();

    assert!(db.embedding_generation_ready("encoder-a", 10).unwrap());

    // Running cleanup when no generation is building
    let cleaned = db.cleanup_stale_generations().unwrap();
    assert_eq!(cleaned, 0, "No building generations to clean up");

    // Existing ready generation must remain ready
    assert!(
        db.embedding_generation_ready("encoder-a", 10).unwrap(),
        "cleanup_stale_generations must not touch ready generations"
    );
    let ready = db.get_latest_ready_generation().unwrap().unwrap();
    assert_eq!(ready.id, gen1);
    assert_eq!(ready.status, EmbeddingGenerationStatus::Ready);
}

/// Challenge 10: Metadata helper invariants (coverage_ratio, is_complete, is_ready).
#[test]
fn challenge_coverage_ratio_and_completion_invariants() {
    let partial = EmbeddingGeneration {
        id: 1,
        encoder_key: "k".to_string(),
        source_revision: 1,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 50,
        created_at: 0,
        updated_at: 0,
    };
    assert!(partial.is_ready());
    assert!(!partial.is_complete());
    assert!((partial.coverage_ratio() - 0.5).abs() < 1e-6);

    let complete = EmbeddingGeneration {
        id: 2,
        encoder_key: "k".to_string(),
        source_revision: 1,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 0,
        updated_at: 0,
    };
    assert!(complete.is_ready());
    assert!(complete.is_complete());
    assert!((complete.coverage_ratio() - 1.0).abs() < 1e-6);

    let zero = EmbeddingGeneration {
        id: 3,
        encoder_key: "k".to_string(),
        source_revision: 1,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Ready,
        eligible_symbols: 0,
        embedded_symbols: 0,
        created_at: 0,
        updated_at: 0,
    };
    assert!(zero.is_ready());
    assert!(zero.is_complete());
    assert!((zero.coverage_ratio() - 1.0).abs() < 1e-6);

    let building = EmbeddingGeneration {
        id: 4,
        encoder_key: "k".to_string(),
        source_revision: 1,
        dimensions: 384,
        status: EmbeddingGenerationStatus::Building,
        eligible_symbols: 100,
        embedded_symbols: 100,
        created_at: 0,
        updated_at: 0,
    };
    assert!(!building.is_ready());
    assert!(!building.is_complete());
}

/// Challenge 11: Watcher incremental vector updates are strictly rejected if no ready
/// generation exists (e.g. while building, failed, superseded) or on dimension mismatch.
#[test]
fn challenge_watcher_update_rejects_unready_generation() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    // 11a: When generation is in Building status, watcher update must be rejected
    let gen_build = db.begin_embedding_generation("encoder-a", 1, 384).unwrap();
    let pair = vec![("sym_w1".to_string(), vec![0.1_f32; 384])];
    let err_building = db.store_file_embeddings_for_ready_generation(
        "src/lib.rs",
        "encoder-a",
        gen_build,
        1,
        &pair,
    );
    assert!(
        err_building.is_err(),
        "Watcher update must be rejected when generation is building"
    );
    assert!(
        err_building
            .unwrap_err()
            .to_string()
            .contains("no ready embedding generation exists")
    );

    // 11b: When generation is in Failed status, watcher update must be rejected
    db.fail_embedding_generation(gen_build).unwrap();
    let err_failed = db.store_file_embeddings_for_ready_generation(
        "src/lib.rs",
        "encoder-a",
        gen_build,
        1,
        &pair,
    );
    assert!(
        err_failed.is_err(),
        "Watcher update must be rejected when generation is failed"
    );
    assert!(
        err_failed
            .unwrap_err()
            .to_string()
            .contains("no ready embedding generation exists")
    );

    // 11c: When generation is Superseded, watcher update must be rejected
    let gen_ready = db.begin_embedding_generation("encoder-a", 2, 384).unwrap();
    db.publish_embedding_generation(gen_ready, 2, 0, 0).unwrap();
    // Begin a new generation which supersedes gen_ready
    let _gen_new = db.begin_embedding_generation("encoder-a", 3, 384).unwrap();
    let err_super = db.store_file_embeddings_for_ready_generation(
        "src/lib.rs",
        "encoder-a",
        gen_ready,
        2,
        &pair,
    );
    assert!(
        err_super.is_err(),
        "Watcher update must be rejected when previous generation is superseded"
    );
    assert!(
        err_super
            .unwrap_err()
            .to_string()
            .contains("no ready embedding generation exists")
    );

    // 11d: When generation is ready, but vector dimension mismatches, reject update
    let gen_ready2 = db.begin_embedding_generation("encoder-a", 4, 384).unwrap();
    db.publish_embedding_generation(gen_ready2, 4, 0, 0)
        .unwrap();
    let mismatched_pair = vec![("sym_w1".to_string(), vec![0.1_f32; 512])];
    let err_dim = db.store_file_embeddings_for_ready_generation(
        "src/lib.rs",
        "encoder-a",
        gen_ready2,
        4,
        &mismatched_pair,
    );
    assert!(
        err_dim.is_err(),
        "Watcher update must be rejected when vector dimension mismatches ready generation"
    );
    assert!(
        err_dim
            .unwrap_err()
            .to_string()
            .contains("dimension mismatch")
    );

    // 11e: When generation is ready, but encoder_key mismatches, reject update
    let err_enc = db.store_file_embeddings_for_ready_generation(
        "src/lib.rs",
        "wrong-encoder",
        gen_ready2,
        4,
        &pair,
    );
    assert!(
        err_enc.is_err(),
        "Watcher update must be rejected when encoder_key mismatches ready generation"
    );
    assert!(
        err_enc
            .unwrap_err()
            .to_string()
            .contains("encoder key mismatch")
    );

    // 11f: When expected source revision mismatches generation revision, reject update
    let err_rev = db.store_file_embeddings_for_ready_generation(
        "src/lib.rs",
        "encoder-a",
        gen_ready2,
        999,
        &pair,
    );
    assert!(
        err_rev.is_err(),
        "Watcher update must be rejected when revision mismatches"
    );
    assert!(
        err_rev
            .unwrap_err()
            .to_string()
            .contains("revision advanced")
    );
}

/// Challenge 12: Watcher vector updates atomically replace stale vectors for the file
/// while preserving other files' vectors, and roll back cleanly on transaction failure.
#[test]
fn challenge_watcher_update_atomic_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db.publish_test_generation("encoder-a", 1, 384).unwrap();
    assert!(db.embedding_generation_ready("encoder-a", 1).unwrap());

    // Insert file metadata for foreign keys
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('src/a.rs', 'rust', 'h1', 10, 0)",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified) VALUES ('src/b.rs', 'rust', 'h2', 10, 0)",
            [],
        )
        .unwrap();

    // Insert initial symbols
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('a1', 'fn_a1', 'function', 'rust', 'src/a.rs')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('a2', 'fn_a2', 'function', 'rust', 'src/a.rs')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('b1', 'fn_b1', 'function', 'rust', 'src/b.rs')",
            [],
        )
        .unwrap();

    // Store initial embeddings for both files
    let batch_a = vec![
        ("a1".to_string(), vec![0.1_f32; 384]),
        ("a2".to_string(), vec![0.2_f32; 384]),
    ];
    let batch_b = vec![("b1".to_string(), vec![0.3_f32; 384])];
    db.store_file_embeddings_for_ready_generation("src/a.rs", "encoder-a", gen_id, 1, &batch_a)
        .unwrap();
    db.store_file_embeddings_for_ready_generation("src/b.rs", "encoder-a", gen_id, 1, &batch_b)
        .unwrap();

    assert!(db.get_embedding("a1").unwrap().is_some());
    assert!(db.get_embedding("a2").unwrap().is_some());
    assert!(db.get_embedding("b1").unwrap().is_some());

    // Atomic update: src/a.rs is modified. a1 is updated, a2 is removed, a3 is added.
    db.conn
        .execute("DELETE FROM symbols WHERE id = 'a2'", [])
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path) VALUES ('a3', 'fn_a3', 'function', 'rust', 'src/a.rs')",
            [],
        )
        .unwrap();

    let updated_batch_a = vec![
        ("a1".to_string(), vec![0.8_f32; 384]),
        ("a3".to_string(), vec![0.9_f32; 384]),
    ];
    db.store_file_embeddings_for_ready_generation(
        "src/a.rs",
        "encoder-a",
        gen_id,
        1,
        &updated_batch_a,
    )
    .unwrap();

    // Verify:
    // 1. a1 has the updated vector
    let emb_a1 = db.get_embedding("a1").unwrap().unwrap();
    assert!((emb_a1[0] - 0.8_f32).abs() < 1e-6);
    // 2. a2 is purged
    assert!(db.get_embedding("a2").unwrap().is_none());
    // 3. a3 is present
    assert!(db.get_embedding("a3").unwrap().is_some());
    // 4. b1 in src/b.rs is completely untouched
    let emb_b1 = db.get_embedding("b1").unwrap().unwrap();
    assert!((emb_b1[0] - 0.3_f32).abs() < 1e-6);

    // Verify rollback on failure: dimension mismatch in src/b.rs leaves b1 untouched
    let bad_batch_b = vec![("b1".to_string(), vec![0.5_f32; 512])];
    assert!(
        db.store_file_embeddings_for_ready_generation(
            "src/b.rs",
            "encoder-a",
            gen_id,
            1,
            &bad_batch_b
        )
        .is_err()
    );
    let emb_b1_after = db.get_embedding("b1").unwrap().unwrap();
    assert!((emb_b1_after[0] - 0.3_f32).abs() < 1e-6);
}

/// Challenge 13: publish_embedding_generation verifies actual stored vector coverage.
#[test]
fn challenge_publish_verifies_actual_vector_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    // 13a: Declaring 1 embedded symbol when 0 vectors are stored must fail
    let gen1 = db.begin_embedding_generation("encoder-a", 1, 384).unwrap();
    let err1 = db.publish_embedding_generation(gen1, 1, 1, 1);
    assert!(err1.is_err());
    assert!(
        err1.unwrap_err()
            .to_string()
            .contains("actual stored vector count")
    );

    // 13b: When eligible_symbols is 0 but vectors exist, must fail
    let gen2 = db.begin_embedding_generation("encoder-a", 2, 384).unwrap();
    let pair = vec![("sym1".to_string(), vec![0.1_f32; 384])];
    db.store_embeddings_for_generation(gen2, &pair).unwrap();
    let err2 = db.publish_embedding_generation(gen2, 2, 0, 1);
    assert!(err2.is_err());
    assert!(
        err2.unwrap_err()
            .to_string()
            .contains("eligible symbols is 0")
    );

    // 13c: When embedded_symbols < eligible_symbols, must reject with incomplete message
    let gen3 = db.begin_embedding_generation("encoder-a", 3, 384).unwrap();
    db.store_embeddings_for_generation(gen3, &pair).unwrap();
    let err3 = db.publish_embedding_generation(gen3, 3, 2, 1);
    assert!(err3.is_err());
    assert!(
        err3.unwrap_err()
            .to_string()
            .contains("Cannot publish incomplete embedding generation")
    );

    // 13d: When actual vector count matches declared embedded count and embedded >= eligible, succeeds
    let gen4 = db.begin_embedding_generation("encoder-a", 4, 384).unwrap();
    db.store_embeddings_for_generation(gen4, &pair).unwrap();
    db.publish_embedding_generation(gen4, 4, 1, 1).unwrap();
    assert!(db.embedding_generation_ready("encoder-a", 4).unwrap());
}

/// Challenge 14: Watcher update advances source_revision to latest canonical revision and updates counts.
#[test]
fn challenge_watcher_advances_source_revision_and_counts() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = SymbolDatabase::new(&dir.path().join("symbols.db")).unwrap();

    let gen_id = db.publish_test_generation("encoder-a", 10, 384).unwrap();

    // Advance canonical revision to 15
    db.conn
        .execute("INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at) VALUES (15, 'ws1', 'incremental', 0)", [])
        .unwrap();

    let batch = vec![("sym_x".to_string(), vec![0.5_f32; 384])];
    db.store_file_embeddings_for_ready_generation("src/x.rs", "encoder-a", gen_id, 10, &batch)
        .unwrap();

    let meta = db.get_embedding_generation(gen_id).unwrap().unwrap();
    assert_eq!(meta.source_revision, 15);
    assert_eq!(meta.embedded_symbols, 1);
    assert_eq!(meta.eligible_symbols, 1);
}
