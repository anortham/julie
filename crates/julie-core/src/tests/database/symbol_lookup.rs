use super::*;

/// 🔴 TDD TEST: This test SHOULD FAIL until get_symbols_by_ids preserves order
///
/// Bug: get_symbols_by_ids() uses WHERE id IN(...) with no ORDER BY clause.
/// SQLite returns rows in arbitrary order, causing semantic search to pair
/// similarity scores with wrong symbols.
///
/// Expected behavior: Results should match input ID order exactly.
#[test]
fn test_get_symbols_by_ids_preserves_order() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("order_test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create symbols with DIFFERENT characteristics to ensure they don't naturally sort to input order
    let symbols = vec![
        Symbol {
            id: "zzz_last".to_string(),         // Alphabetically last
            name: "alpha_function".to_string(), // But name is first
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/test.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 10,
            start_byte: 0,
            end_byte: 10,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        Symbol {
            id: "mmm_middle".to_string(),
            name: "beta_function".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/test.rs".to_string(),
            start_line: 10,
            start_column: 0,
            end_line: 10,
            end_column: 10,
            start_byte: 100,
            end_byte: 110,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        Symbol {
            id: "aaa_first".to_string(),       // Alphabetically first
            name: "zeta_function".to_string(), // But name is last
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/test.rs".to_string(),
            start_line: 20,
            start_column: 0,
            end_line: 20,
            end_column: 10,
            start_byte: 200,
            end_byte: 210,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        Symbol {
            id: "ppp_fourth".to_string(),
            name: "delta_function".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/test.rs".to_string(),
            start_line: 30,
            start_column: 0,
            end_line: 30,
            end_column: 10,
            start_byte: 300,
            end_byte: 310,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
    ];

    // Store all symbols
    db.bulk_store_symbols(&symbols, "test_workspace").unwrap();

    // Request in specific order (NOT alphabetical by ID or name or line number)
    let requested_order = vec![
        "ppp_fourth".to_string(), // 4th alphabetically
        "aaa_first".to_string(),  // 1st alphabetically
        "zzz_last".to_string(),   // Last alphabetically
        "mmm_middle".to_string(), // Middle alphabetically
    ];

    // Retrieve symbols
    let results = db.get_symbols_by_ids(&requested_order).unwrap();

    // CRITICAL ASSERTION: Results MUST match input order exactly
    assert_eq!(
        results.len(),
        requested_order.len(),
        "Should return all requested symbols"
    );

    for (i, symbol) in results.iter().enumerate() {
        assert_eq!(
            symbol.id, requested_order[i],
            "Symbol at position {} should be '{}' but got '{}'. \
             This means get_symbols_by_ids() is not preserving input order, \
             which causes semantic search to pair similarity scores with wrong symbols!",
            i, requested_order[i], symbol.id
        );
    }

    println!("✅ get_symbols_by_ids() preserves input order correctly");
}

#[test]
fn test_get_symbols_by_ids_handles_over_32k_ids() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("large_batch.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let real_symbols: Vec<Symbol> = (0..10)
        .map(|i| Symbol {
            id: format!("sym_{i:05}"),
            name: format!("func_{i}"),
            kind: SymbolKind::Function,
            language: "python".to_string(),
            file_path: "main.py".to_string(),
            start_line: i as u32,
            start_column: 0,
            end_line: i as u32 + 1,
            end_column: 0,
            start_byte: 0,
            end_byte: 10,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        })
        .collect();
    db.bulk_store_symbols(&real_symbols, "test_workspace")
        .unwrap();

    let mut ids: Vec<String> = (0..33000).map(|i| format!("nonexistent_{i:06}")).collect();
    ids.push("sym_00005".to_string());
    ids.push("sym_00002".to_string());
    ids.push("sym_00008".to_string());

    let results = db.get_symbols_by_ids(&ids).unwrap();

    assert_eq!(results.len(), 3, "should return the 3 matching symbols");
    assert_eq!(results[0].id, "sym_00005");
    assert_eq!(results[1].id, "sym_00002");
    assert_eq!(results[2].id, "sym_00008");
}
