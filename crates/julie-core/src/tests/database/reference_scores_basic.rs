use super::*;

/// Task 2: Verify compute_reference_scores applies correct weights
#[test]
fn test_compute_reference_scores_weighted() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert a file
    db.store_file_info(&FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc123".to_string(),
        size: 100,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 4,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Insert symbols: target + 3 sources
    let symbols: Vec<_> = [
        ("target", "TargetFn"),
        ("caller1", "Caller1"),
        ("caller2", "Caller2"),
        ("caller3", "Caller3"),
    ]
    .iter()
    .map(|(id, name)| {
        symbol_builder(*id, *name, "test.rs")
            .kind(SymbolKind::Function)
            .language("rust")
            .span(1, 0, 10, 1)
            .bytes(0, 100)
            .confidence(1.0)
            .build()
    })
    .collect();
    db.store_symbols(&symbols).unwrap();

    // Insert relationships TO target with different kinds:
    // caller1 --calls--> target (weight 3)
    // caller2 --imports--> target (weight 2)
    // caller3 --uses--> target (weight 1)
    let relationships: Vec<_> = [
        ("r1", "caller1", RelationshipKind::Calls),
        ("r2", "caller2", RelationshipKind::Imports),
        ("r3", "caller3", RelationshipKind::Uses),
    ]
    .iter()
    .map(|(rel_id, from_id, kind)| {
        relationship_builder(*rel_id, *from_id, "target")
            .kind(kind.clone())
            .line_number(0)
            .build()
    })
    .collect();
    db.store_relationships(&relationships).unwrap();

    // Compute scores
    db.compute_reference_scores().unwrap();

    // Verify target score = 3 + 2 + 1 = 6.0
    let target_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'target'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (target_score - 6.0).abs() < f64::EPSILON,
        "target reference_score should be 6.0 (calls=3 + imports=2 + uses=1), got {}",
        target_score
    );

    // Verify callers have score 0.0 (no incoming refs)
    for caller_id in ["caller1", "caller2", "caller3"] {
        let score: f64 = db
            .conn
            .query_row(
                "SELECT reference_score FROM symbols WHERE id = ?1",
                rusqlite::params![caller_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            (score - 0.0).abs() < f64::EPSILON,
            "{} should have reference_score 0.0 (no incoming refs), got {}",
            caller_id,
            score
        );
    }
}

/// Task 2: Verify self-references (recursion) are excluded from scoring
#[test]
fn test_compute_reference_scores_excludes_self_refs() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert a file
    db.store_file_info(&FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc123".to_string(),
        size: 100,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Insert a single symbol
    db.store_symbols(&[symbol_builder("recursive_fn", "factorial", "test.rs")
        .kind(SymbolKind::Function)
        .language("rust")
        .span(1, 0, 10, 1)
        .bytes(0, 100)
        .confidence(1.0)
        .build()])
        .unwrap();

    // Insert self-referencing relationship (recursion)
    db.store_relationships(&[
        relationship_builder("r_self", "recursive_fn", "recursive_fn")
            .kind(RelationshipKind::Calls)
            .line_number(0)
            .build(),
    ])
    .unwrap();

    // Compute scores
    db.compute_reference_scores().unwrap();

    // Self-reference should be excluded, score should be 0.0
    let score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'recursive_fn'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (score - 0.0).abs() < f64::EPSILON,
        "Self-referencing symbol should have reference_score 0.0, got {}",
        score
    );
}

/// Task 4: Batch query for reference scores
#[test]
fn test_get_reference_scores_batch() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert a file (foreign key requirement)
    db.store_file_info(&FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc123".to_string(),
        size: 100,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 4,
        line_count: 0,
        content: None,
    })
    .unwrap();

    let score_rows = [
        ("s1", "fn_a", 5.0),
        ("s2", "fn_b", 0.0),
        ("s3", "fn_c", 12.5),
        ("s4", "fn_d", 3.0),
    ];
    let symbols: Vec<_> = score_rows
        .iter()
        .map(|(id, name, _score)| {
            symbol_builder(*id, *name, "test.rs")
                .kind(SymbolKind::Function)
                .language("rust")
                .span(1, 0, 10, 1)
                .bytes(0, 100)
                .confidence(1.0)
                .build()
        })
        .collect();
    db.store_symbols(&symbols).unwrap();
    let score_updates: Vec<_> = score_rows
        .iter()
        .map(|(id, _name, score)| (*id, *score))
        .collect();
    set_symbol_reference_scores(&db, &score_updates).unwrap();

    // Case 1: All IDs found — correct scores returned
    let ids = vec!["s1", "s2", "s3", "s4"];
    let scores = db.get_reference_scores(&ids).unwrap();
    assert_eq!(scores.len(), 4);
    assert!((scores["s1"] - 5.0).abs() < f64::EPSILON);
    assert!((scores["s2"] - 0.0).abs() < f64::EPSILON);
    assert!((scores["s3"] - 12.5).abs() < f64::EPSILON);
    assert!((scores["s4"] - 3.0).abs() < f64::EPSILON);

    // Case 2: Some IDs not found — only found ones in HashMap
    let partial_ids = vec!["s1", "s999", "s3"];
    let partial_scores = db.get_reference_scores(&partial_ids).unwrap();
    assert_eq!(partial_scores.len(), 2);
    assert!((partial_scores["s1"] - 5.0).abs() < f64::EPSILON);
    assert!((partial_scores["s3"] - 12.5).abs() < f64::EPSILON);
    assert!(!partial_scores.contains_key("s999"));

    // Case 3: Empty input — empty HashMap
    let empty_ids: Vec<&str> = vec![];
    let empty_scores = db.get_reference_scores(&empty_ids).unwrap();
    assert!(empty_scores.is_empty());
}

/// Task 2: Verify symbols with only outgoing refs have score 0.0
#[test]
fn test_compute_reference_scores_zero_for_no_incoming() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert a file
    db.store_file_info(&FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc123".to_string(),
        size: 100,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 2,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Insert two symbols
    let symbols: Vec<_> = [("sender", "send_data"), ("receiver", "receive_data")]
        .iter()
        .map(|(id, name)| {
            symbol_builder(*id, *name, "test.rs")
                .kind(SymbolKind::Function)
                .language("rust")
                .span(1, 0, 10, 1)
                .bytes(0, 100)
                .confidence(1.0)
                .build()
        })
        .collect();
    db.store_symbols(&symbols).unwrap();

    // sender --calls--> receiver (sender has outgoing, no incoming)
    db.store_relationships(&[relationship_builder("r1", "sender", "receiver")
        .kind(RelationshipKind::Calls)
        .line_number(0)
        .build()])
        .unwrap();

    // Compute scores
    db.compute_reference_scores().unwrap();

    // sender has outgoing only, no incoming => score 0.0
    let sender_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'sender'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (sender_score - 0.0).abs() < f64::EPSILON,
        "Symbol with only outgoing refs should have reference_score 0.0, got {}",
        sender_score
    );

    // receiver has incoming calls => score 3.0
    let receiver_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'receiver'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (receiver_score - 3.0).abs() < f64::EPSILON,
        "receiver should have reference_score 3.0 (one call), got {}",
        receiver_score
    );
}
/// Verify get_reference_scores batches correctly with >900 IDs (SQLite bind param limit).
#[test]
fn test_get_reference_scores_large_batch() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert a file (foreign key requirement)
    db.store_file_info(&FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc123".to_string(),
        size: 100,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 0,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Insert 1500 symbols — enough to require two batches (900 + 600).
    let count = 1500usize;
    let score_updates: Vec<(String, f64)> = (0..count)
        .map(|i| (format!("sym_{i}"), (i % 50) as f64))
        .collect();
    let symbols: Vec<_> = score_updates
        .iter()
        .enumerate()
        .map(|(i, (id, _score))| {
            symbol_builder(id.as_str(), format!("fn_{i}"), "test.rs")
                .kind(SymbolKind::Function)
                .language("rust")
                .span(1, 0, 10, 1)
                .bytes(0, 100)
                .confidence(1.0)
                .build()
        })
        .collect();
    db.store_symbols(&symbols).unwrap();
    let score_refs: Vec<_> = score_updates
        .iter()
        .map(|(id, score)| (id.as_str(), *score))
        .collect();
    set_symbol_reference_scores(&db, &score_refs).unwrap();

    // Query all 1500 IDs — this would fail pre-batching with SQLite bind limit.
    let ids: Vec<String> = (0..count).map(|i| format!("sym_{i}")).collect();
    let id_refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
    let scores = db.get_reference_scores(&id_refs).unwrap();

    assert_eq!(scores.len(), count);
    for i in 0..count {
        let expected = (i % 50) as f64;
        let actual = scores[&format!("sym_{i}")];
        assert!(
            (actual - expected).abs() < f64::EPSILON,
            "sym_{i}: expected {expected}, got {actual}"
        );
    }

    // Also query with a mix of existing and non-existing IDs across batch boundaries.
    let mut mixed_ids: Vec<String> = (0..900).map(|i| format!("sym_{i}")).collect();
    mixed_ids.extend((0..600).map(|i| format!("nonexistent_{i}")));
    mixed_ids.extend((900..count).map(|i| format!("sym_{i}")));
    let mixed_refs: Vec<&str> = mixed_ids.iter().map(|s| s.as_str()).collect();
    let mixed_scores = db.get_reference_scores(&mixed_refs).unwrap();

    // Only the 1500 real symbols should be in the result, not the 600 fake ones.
    assert_eq!(mixed_scores.len(), count);
    assert!(!mixed_scores.contains_key("nonexistent_0"));
}
