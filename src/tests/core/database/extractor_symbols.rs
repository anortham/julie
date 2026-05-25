use super::*;

#[tokio::test]
async fn test_extractor_database_integration() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Simulate what an extractor would create
    use crate::extractors::base::BaseExtractor;

    let source_code = r#"
        function getUserById(id: string): Promise<User> {
            return fetchUser(id);
        }
        "#;

    // This test will initially fail - we need to verify extractors can create symbols
    // with the new field structure that work with the database
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let base_extractor = BaseExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        source_code.to_string(),
        &workspace_root,
    );

    // Create a symbol like an extractor would
    let mut metadata = HashMap::new();
    metadata.insert("isAsync".to_string(), serde_json::Value::Bool(false));
    metadata.insert(
        "returnType".to_string(),
        serde_json::Value::String("Promise<User>".to_string()),
    );

    let symbol = Symbol {
        id: base_extractor.generate_id("getUserById", 2, 8),
        name: "getUserById".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: "test.ts".to_string(),
        start_line: 2,
        start_column: 8,
        end_line: 4,
        end_column: 9,
        start_byte: 0,
        end_byte: 0,
        signature: Some("function getUserById(id: string): Promise<User>".to_string()),
        doc_comment: None,
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: None,
        metadata: Some(metadata),
        semantic_group: None, // Will be populated during cross-language analysis
        confidence: None,     // Will be calculated based on parsing context
        code_context: None,
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    };

    // Following foreign key contract: store file record first
    let file_info = FileInfo {
        path: "test.ts".to_string(),
        language: "typescript".to_string(),
        hash: "test-ts-hash".to_string(),
        size: 150,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    };
    db.store_file_info(&file_info).unwrap();

    // Test that extractor-generated symbols work with database
    db.store_symbols_transactional(&[symbol.clone()]).unwrap();

    let retrieved = db.get_symbol_by_id(&symbol.id).unwrap().unwrap();
    assert_eq!(retrieved.name, "getUserById");
    assert!(retrieved.metadata.is_some());

    let metadata = retrieved.metadata.unwrap();
    assert_eq!(
        metadata.get("returnType").unwrap().as_str().unwrap(),
        "Promise<User>"
    );
}

/// 🔴 TDD TEST: This test SHOULD FAIL until schema is complete
/// Tests that all missing database fields are properly persisted and retrieved
#[tokio::test]
async fn test_complete_symbol_field_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("complete_fields.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create file record first (FK requirement)
    let file_info = FileInfo {
        path: "complete_test.rs".to_string(),
        language: "rust".to_string(),
        hash: "complete-hash".to_string(),
        size: 500,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    };
    db.store_file_info(&file_info).unwrap();

    // Create symbol with ALL fields populated (including the missing ones)
    let symbol = Symbol {
        id: "complete-symbol-id".to_string(),
        name: "complete_function".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "complete_test.rs".to_string(),
        start_line: 10,
        start_column: 4,
        end_line: 20,
        end_column: 5,
        // 🔴 THESE FIELDS ARE CURRENTLY LOST (not in database schema):
        start_byte: 150,
        end_byte: 450,
        doc_comment: Some("/// This function does something important".to_string()),
        visibility: Some(crate::extractors::base::Visibility::Public),
        code_context: Some(
            "  // line before\n  fn complete_function() {\n  // line after".to_string(),
        ),
        content_type: None,
        body_span: Some(crate::extractors::base::NormalizedSpan {
            start_line: 12,
            start_column: 4,
            end_line: 18,
            end_column: 5,
            start_byte: 180,
            end_byte: 420,
        }),
        body_hash: Some("hash:complete-function-body".to_string()),
        // Regular fields that work:
        signature: Some("fn complete_function() -> Result<()>".to_string()),
        parent_id: None,
        metadata: None,
        semantic_group: Some("test-group".to_string()),
        confidence: Some(0.95),
        annotations: Vec::new(),
    };

    // Store the symbol
    db.store_symbols_transactional(&[symbol.clone()]).unwrap();

    // Retrieve and verify ALL fields are preserved
    let retrieved = db
        .get_symbol_by_id("complete-symbol-id")
        .unwrap()
        .expect("Symbol should exist in database");

    // Basic fields (these already work)
    assert_eq!(retrieved.name, "complete_function");
    assert_eq!(retrieved.start_line, 10);
    assert_eq!(retrieved.end_line, 20);

    // 🔴 CRITICAL MISSING FIELDS - These assertions will FAIL until schema is fixed:
    assert_eq!(retrieved.start_byte, 150, "start_byte should be persisted");
    assert_eq!(retrieved.end_byte, 450, "end_byte should be persisted");
    assert_eq!(
        retrieved.doc_comment,
        Some("/// This function does something important".to_string()),
        "doc_comment should be persisted"
    );
    assert_eq!(
        retrieved.visibility,
        Some(crate::extractors::base::Visibility::Public),
        "visibility should be persisted"
    );
    assert_eq!(
        retrieved.code_context,
        Some("  // line before\n  fn complete_function() {\n  // line after".to_string()),
        "code_context should be persisted"
    );
    assert_eq!(
        retrieved.body_span,
        Some(crate::extractors::base::NormalizedSpan {
            start_line: 12,
            start_column: 4,
            end_line: 18,
            end_column: 5,
            start_byte: 180,
            end_byte: 420,
        }),
        "body_span should be persisted"
    );
    assert_eq!(
        retrieved.body_hash,
        Some("hash:complete-function-body".to_string()),
        "body_hash should be persisted"
    );

    println!("✅ ALL FIELDS PERSISTED CORRECTLY!");
}
