use super::*;

#[tokio::test]
async fn test_relationship_with_id_field() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Following foreign key contract: create file and symbols first
    let file_info = FileInfo {
        path: "main.rs".to_string(),
        language: "rust".to_string(),
        hash: "main-hash".to_string(),
        size: 500,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 2,
        line_count: 0,
        content: None,
    };
    db.store_file_info(&file_info).unwrap();

    let caller_symbol = Symbol {
        id: "caller_func".to_string(),
        name: "caller_func".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "main.rs".to_string(),
        start_line: 10,
        start_column: 0,
        end_line: 15,
        end_column: 1,
        start_byte: 0,
        end_byte: 0,
        signature: Some("fn caller_func()".to_string()),
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
    };

    let called_symbol = Symbol {
        id: "called_func".to_string(),
        name: "called_func".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "main.rs".to_string(),
        start_line: 20,
        start_column: 0,
        end_line: 25,
        end_column: 1,
        start_byte: 0,
        end_byte: 0,
        signature: Some("fn called_func()".to_string()),
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
    };

    db.store_symbols_transactional(&[caller_symbol, called_symbol])
        .unwrap();

    // Create relationship with generated id
    let relationship = crate::extractors::base::Relationship {
        id: "caller_func_called_func_Calls_42".to_string(),
        from_symbol_id: "caller_func".to_string(),
        to_symbol_id: "called_func".to_string(),
        kind: crate::extractors::base::RelationshipKind::Calls,
        file_path: "main.rs".to_string(),
        line_number: 42,
        confidence: 0.9,
        metadata: None,
    };

    // Store the relationship
    db.store_relationships(&[relationship.clone()]).unwrap();

    // Retrieve relationships for the from_symbol
    let relationships = db.get_outgoing_relationships("caller_func").unwrap();
    assert_eq!(relationships.len(), 1);

    let retrieved = &relationships[0];
    assert_eq!(retrieved.id, "caller_func_called_func_Calls_42");
    assert_eq!(retrieved.from_symbol_id, "caller_func");
    assert_eq!(retrieved.to_symbol_id, "called_func");
    assert_eq!(retrieved.confidence, 0.9);
}

#[tokio::test]
async fn test_cross_language_semantic_grouping() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create symbols from different languages but same semantic group
    let ts_interface = Symbol {
        id: "ts-user-interface".to_string(),
        name: "User".to_string(),
        kind: SymbolKind::Interface,
        language: "typescript".to_string(),
        file_path: "user.ts".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 10,
        end_column: 1,
        start_byte: 0,
        end_byte: 200,
        signature: Some("interface User".to_string()),
        doc_comment: None,
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: None,
        metadata: None,
        semantic_group: Some("user-entity".to_string()),
        confidence: Some(1.0),
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    };

    let rust_struct = Symbol {
        id: "rust-user-struct".to_string(),
        name: "User".to_string(),
        kind: SymbolKind::Struct,
        language: "rust".to_string(),
        file_path: "user.rs".to_string(),
        start_line: 5,
        start_column: 0,
        end_line: 15,
        end_column: 1,
        start_byte: 100,
        end_byte: 400,
        signature: Some("struct User".to_string()),
        doc_comment: None,
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: None,
        metadata: None,
        semantic_group: Some("user-entity".to_string()),
        confidence: Some(0.98),
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    };

    // Following foreign key contract: store file records first
    let ts_file_info = FileInfo {
        path: "user.ts".to_string(),
        language: "typescript".to_string(),
        hash: "ts-hash".to_string(),
        size: 200,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    };
    db.store_file_info(&ts_file_info).unwrap();

    let rust_file_info = FileInfo {
        path: "user.rs".to_string(),
        language: "rust".to_string(),
        hash: "rust-hash".to_string(),
        size: 300,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    };
    db.store_file_info(&rust_file_info).unwrap();

    // Store both symbols
    db.store_symbols_transactional(&[ts_interface, rust_struct])
        .unwrap();

    // Query symbols by semantic group (this will fail initially - need to implement)
    let grouped_symbols = db.get_symbols_by_semantic_group("user-entity").unwrap();
    assert_eq!(grouped_symbols.len(), 2);

    // Verify we have both TypeScript and Rust symbols
    let languages: std::collections::HashSet<_> = grouped_symbols
        .iter()
        .map(|s| s.language.as_str())
        .collect();
    assert!(languages.contains("typescript"));
    assert!(languages.contains("rust"));
}

#[tokio::test]
async fn test_get_outgoing_relationships_for_symbols_batch() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let file_info = FileInfo {
        path: "main.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash-main".to_string(),
        size: 100,
        last_modified: 12345,
        last_indexed: 0,
        symbol_count: 4,
        line_count: 0,
        content: None,
    };
    db.store_file_info(&file_info).unwrap();

    let mk_symbol = |id: &str, name: &str| Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "main.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 1,
        start_byte: 0,
        end_byte: 0,
        signature: Some(format!("fn {}()", name)),
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
    };

    db.store_symbols_transactional(&[
        mk_symbol("caller_a", "caller_a"),
        mk_symbol("caller_b", "caller_b"),
        mk_symbol("callee_x", "callee_x"),
        mk_symbol("callee_y", "callee_y"),
    ])
    .unwrap();

    let relationships = vec![
        crate::extractors::Relationship {
            id: "rel_a_x".to_string(),
            from_symbol_id: "caller_a".to_string(),
            to_symbol_id: "callee_x".to_string(),
            kind: crate::extractors::RelationshipKind::Calls,
            file_path: "main.rs".to_string(),
            line_number: 10,
            confidence: 1.0,
            metadata: None,
        },
        crate::extractors::Relationship {
            id: "rel_b_y".to_string(),
            from_symbol_id: "caller_b".to_string(),
            to_symbol_id: "callee_y".to_string(),
            kind: crate::extractors::RelationshipKind::Calls,
            file_path: "main.rs".to_string(),
            line_number: 20,
            confidence: 1.0,
            metadata: None,
        },
    ];
    db.store_relationships(&relationships).unwrap();

    let caller_ids = vec!["caller_a".to_string(), "caller_b".to_string()];
    let outgoing = db
        .get_outgoing_relationships_for_symbols(&caller_ids)
        .unwrap();

    assert_eq!(
        outgoing.len(),
        2,
        "batch outgoing lookup should return both relationships"
    );
    assert!(
        outgoing.iter().any(|r| r.id == "rel_a_x"),
        "expected relationship from caller_a"
    );
    assert!(
        outgoing.iter().any(|r| r.id == "rel_b_y"),
        "expected relationship from caller_b"
    );
}
