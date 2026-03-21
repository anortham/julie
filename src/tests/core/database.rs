// Tests extracted from src/database/mod.rs
// These were previously inline tests that have been moved to follow project standards

use crate::database::*;
use crate::extractors::{Symbol, SymbolKind};
use crate::tests::test_helpers::open_test_connection;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;
use tree_sitter::Parser;

#[tokio::test]
async fn test_database_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();
    let stats = db.get_stats().unwrap();

    assert_eq!(stats.total_symbols, 0);
    assert_eq!(stats.total_relationships, 0);
    assert_eq!(stats.total_files, 0);
}

#[test]
fn test_minimal_database_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("minimal.db");

    // Test just the SQLite connection
    let conn = open_test_connection(&db_path).unwrap();

    // Test a simple table creation
    let result = conn.execute("CREATE TABLE test (id TEXT PRIMARY KEY, name TEXT)", []);

    // This should work without "Execute returned results" error
    assert!(result.is_ok());

    // Test a simple insert
    let insert_result = conn.execute("INSERT INTO test VALUES ('1', 'test')", []);
    assert!(insert_result.is_ok());
}

#[tokio::test]
async fn test_debug_foreign_key_constraint() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("debug.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create a temporary file
    let test_file = temp_dir.path().join("test.ts");
    std::fs::write(&test_file, "// test content").unwrap();

    // Store file info
    let file_info =
        crate::database::create_file_info(&test_file, "typescript", temp_dir.path()).unwrap();
    println!("File path in file_info: {}", file_info.path);
    db.store_file_info(&file_info).unwrap();

    // Create a symbol with the same file path (relative to match file_info)
    let file_path =
        crate::utils::paths::to_relative_unix_style(&test_file, temp_dir.path()).unwrap();
    println!("File path in symbol: {}", file_path);

    let symbol = Symbol {
        id: "test-symbol".to_string(),
        name: "testFunction".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: file_path,
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
    };

    // This should work without foreign key constraint error
    let result = db.store_symbols_transactional(&[symbol]);
    assert!(
        result.is_ok(),
        "Foreign key constraint failed: {:?}",
        result
    );
}

#[test]
fn test_individual_table_creation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("individual.db");

    // Create a SymbolDatabase instance manually to test each table individually
    let conn = open_test_connection(&db_path).unwrap();
    let db = SymbolDatabase {
        conn,
        file_path: db_path,
    };

    // Test files table creation
    let files_result = db.create_files_table();
    assert!(
        files_result.is_ok(),
        "Files table creation failed: {:?}",
        files_result
    );

    // Test symbols table creation
    let symbols_result = db.create_symbols_table();
    assert!(
        symbols_result.is_ok(),
        "Symbols table creation failed: {:?}",
        symbols_result
    );

    // Test relationships table creation
    let relationships_result = db.create_relationships_table();
    assert!(
        relationships_result.is_ok(),
        "Relationships table creation failed: {:?}",
        relationships_result
    );
}

#[tokio::test]
async fn test_file_info_storage() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let file_info = FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "abcd1234".to_string(),
        size: 1024,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 5,
        line_count: 0,
        content: None,
    };

    db.store_file_info(&file_info).unwrap();

    let hash = db.get_file_hash("test.rs").unwrap();
    assert_eq!(hash, Some("abcd1234".to_string()));
}

#[tokio::test]
async fn test_symbol_storage_and_retrieval() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let symbol = Symbol {
        id: "test-symbol-1".to_string(),
        name: "test_function".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "test.rs".to_string(),
        start_line: 10,
        start_column: 0,
        end_line: 15,
        end_column: 1,
        start_byte: 0,
        end_byte: 0,
        signature: Some("fn test_function()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    };

    // Following foreign key contract: store file record first
    let file_info = FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "test-hash".to_string(),
        size: 100,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    };
    db.store_file_info(&file_info).unwrap();

    db.store_symbols_transactional(&[symbol.clone()]).unwrap();

    let retrieved = db.get_symbol_by_id("test-symbol-1").unwrap();
    assert!(retrieved.is_some());

    let retrieved_symbol = retrieved.unwrap();
    assert_eq!(retrieved_symbol.name, "test_function");
    assert_eq!(retrieved_symbol.language, "rust");
}

#[test]
fn test_bulk_store_symbols_for_existing_file_paths() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("bulk.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Use a real Go fixture to mirror the production failure scenario
    let fixture_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/real-world/go/main.go");
    let fixture_content = std::fs::read_to_string(&fixture_path).unwrap();

    let workspace_root = fixture_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/tmp/test"));
    let file_info = crate::database::create_file_info(&fixture_path, "go", workspace_root).unwrap();
    db.bulk_store_files(&[file_info]).unwrap();

    let mut parser = Parser::new();
    let go_lang = crate::language::get_tree_sitter_language("go").unwrap();
    parser.set_language(&go_lang).unwrap();
    let tree = parser.parse(&fixture_content, None).unwrap();
    let mut extractor = crate::extractors::go::GoExtractor::new(
        "go".to_string(),
        fixture_path.to_string_lossy().to_string(),
        fixture_content,
        workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);

    assert!(!symbols.is_empty(), "Expected fixture to produce symbols");

    let result = db.bulk_store_symbols(&symbols, "test_workspace");
    assert!(
        result.is_ok(),
        "Bulk store should succeed without foreign key violations: {:?}",
        result
    );
}

#[tokio::test]
async fn test_symbol_with_metadata_and_semantic_fields() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create a temporary file for the test
    let test_file = temp_dir.path().join("user.ts");
    std::fs::write(&test_file, "// test file content").unwrap();

    // Create symbol with all new fields populated
    let mut metadata = HashMap::new();
    metadata.insert("isAsync".to_string(), serde_json::Value::Bool(true));
    metadata.insert(
        "returnType".to_string(),
        serde_json::Value::String("Promise<User>".to_string()),
    );

    let symbol = Symbol {
        id: "test-symbol-complex".to_string(),
        name: "getUserAsync".to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: crate::utils::paths::to_relative_unix_style(&test_file, temp_dir.path())
            .unwrap(),
        start_line: 20,
        start_column: 4,
        end_line: 30,
        end_column: 1,
        start_byte: 500,
        end_byte: 800,
        signature: Some("async getUserAsync(id: string): Promise<User>".to_string()),
        doc_comment: Some("Fetches user data asynchronously".to_string()),
        visibility: Some(crate::extractors::base::Visibility::Public),
        parent_id: None, // No parent for this test
        metadata: Some(metadata.clone()),
        semantic_group: Some("user-data-access".to_string()),
        confidence: Some(0.95),
        code_context: None,
        content_type: None,
    };

    // First, store the file record (required due to foreign key constraint)
    let file_info =
        crate::database::create_file_info(&test_file, "typescript", temp_dir.path()).unwrap();
    println!("DEBUG: File path in file_info: {}", file_info.path);
    println!("DEBUG: Symbol file path: {}", symbol.file_path);
    db.store_file_info(&file_info).unwrap();

    // Store the symbol
    db.store_symbols_transactional(&[symbol.clone()]).unwrap();

    // Retrieve and verify all fields are preserved
    let retrieved = db.get_symbol_by_id("test-symbol-complex").unwrap().unwrap();

    assert_eq!(retrieved.name, "getUserAsync");
    assert_eq!(
        retrieved.semantic_group,
        Some("user-data-access".to_string())
    );
    assert_eq!(retrieved.confidence, Some(0.95));

    // Verify metadata is properly stored and retrieved
    let retrieved_metadata = retrieved.metadata.unwrap();
    assert_eq!(
        retrieved_metadata
            .get("isAsync")
            .unwrap()
            .as_bool()
            .unwrap(),
        true
    );
    assert_eq!(
        retrieved_metadata
            .get("returnType")
            .unwrap()
            .as_str()
            .unwrap(),
        "Promise<User>"
    );
}

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
    let relationships = db.get_relationships_for_symbol("caller_func").unwrap();
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
        // Regular fields that work:
        signature: Some("fn complete_function() -> Result<()>".to_string()),
        parent_id: None,
        metadata: None,
        semantic_group: Some("test-group".to_string()),
        confidence: Some(0.95),
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

    println!("✅ ALL FIELDS PERSISTED CORRECTLY!");
}

// ========================================
// CASCADE ARCHITECTURE: Phase 1 TDD Tests
// ========================================

#[test]
fn test_store_file_with_content() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_with_content(
        "test.md",
        "markdown",
        "abc123",
        1024,
        1234567890,
        "# Test\nThis is test content",
        "test_workspace",
    )
    .unwrap();

    let content = db.get_file_content("test.md").unwrap();
    assert_eq!(content, Some("# Test\nThis is test content".to_string()));
}

// ============================================================
// SCHEMA MIGRATION TESTS
// ============================================================

#[test]
fn test_migration_fresh_database_at_latest_version() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Fresh database should be at latest version
    let version = db.get_schema_version().unwrap();
    assert_eq!(version, LATEST_SCHEMA_VERSION);
}

#[test]
fn test_migration_version_table_exists() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify schema_version table exists
    let result: Result<i64, rusqlite::Error> =
        db.conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0));

    assert!(result.is_ok(), "schema_version table should exist");
}

#[test]
fn test_migration_adds_content_column() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify content column exists in files table
    let has_content = db.has_column("files", "content").unwrap();
    assert!(
        has_content,
        "files table should have content column after migration"
    );
}

#[test]
fn test_migration_from_legacy_v1_database() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create a legacy V1 database (without content column)
    {
        let conn = open_test_connection(&db_path).unwrap();
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();

        // Create old schema WITHOUT content column
        conn.execute(
            "CREATE TABLE files (
                path TEXT PRIMARY KEY,
                language TEXT NOT NULL,
                hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                last_modified INTEGER NOT NULL,
                last_indexed INTEGER DEFAULT 0,
                parse_cache BLOB,
                symbol_count INTEGER DEFAULT 0,
                workspace_id TEXT NOT NULL DEFAULT 'primary'
            )",
            [],
        )
        .unwrap();

        // Insert test data
        conn.execute(
            "INSERT INTO files (path, language, hash, size, last_modified)
             VALUES ('test.rs', 'rust', 'abc123', 1024, 1234567890)",
            [],
        )
        .unwrap();
    }

    // Now open with new code - should trigger migration
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify migration occurred
    let version = db.get_schema_version().unwrap();
    assert_eq!(
        version, LATEST_SCHEMA_VERSION,
        "Database should be migrated to latest version"
    );

    // Verify content column exists
    let has_content = db.has_column("files", "content").unwrap();
    assert!(has_content, "Migration should have added content column");

    // Verify existing data is preserved
    let file_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE path = 'test.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        file_count, 1,
        "Existing data should be preserved after migration"
    );
}

#[test]
fn test_migration_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create database (runs migrations)
    {
        let _db = SymbolDatabase::new(&db_path).unwrap();
    }

    // Open again (should handle already-migrated database)
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();
    let version = db.get_schema_version().unwrap();
    assert_eq!(version, LATEST_SCHEMA_VERSION);

    // Should not error or change version
    let has_content = db.has_column("files", "content").unwrap();
    assert!(has_content);
}

// ============================================================================
// Concurrent Access Tests - Stress Testing for Database Corruption Bug
// ============================================================================

#[test]
fn test_concurrent_read_access_no_corruption() {
    use crate::tests::test_helpers::open_test_connection;
    use std::sync::Arc;
    use std::thread;

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create and populate database
    {
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        // Insert test data
        let symbols = vec![Symbol {
            id: "sym1".to_string(),
            name: "TestFunction".to_string(),
            kind: SymbolKind::Function,
            file_path: "test.rs".to_string(),
            start_line: 1,
            end_line: 10,
            start_column: 0,
            end_column: 1,
            start_byte: 0,
            end_byte: 100,
            signature: Some("fn test()".to_string()),
            doc_comment: None,
            parent_id: None,
            language: "rust".to_string(),
            visibility: Some(crate::extractors::base::types::Visibility::Public),
            metadata: Default::default(),
            code_context: None,
            content_type: None,
            confidence: None,
            semantic_group: None,
        }];

        db.bulk_store_symbols(&symbols, "test_workspace").unwrap();
    }

    // Concurrent read stress test - 10 threads reading simultaneously
    let db_path = Arc::new(db_path);
    let mut handles = vec![];

    for i in 0..10 {
        let db_path = Arc::clone(&db_path);
        let handle = thread::spawn(move || {
            // Each thread opens its own connection with proper configuration
            let conn = open_test_connection(db_path.as_path()).expect("Failed to open connection");

            // Perform multiple read operations
            for j in 0..50 {
                // Query symbols
                let count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
                    .expect(&format!(
                        "Thread {} iteration {} failed to count symbols",
                        i, j
                    ));

                assert_eq!(count, 1, "Thread {} iteration {} got wrong count", i, j);
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    println!("✅ Concurrent read stress test passed: 10 threads × 50 iterations = 500 operations");
}

#[test]
fn test_concurrent_mixed_access_no_corruption() {
    use crate::tests::test_helpers::open_test_connection;
    use std::sync::{Arc, Mutex};
    use std::thread;

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create initial database
    {
        let db = SymbolDatabase::new(&db_path).unwrap();
        drop(db);
    }

    // Wrap db_path in Arc for sharing between threads
    let db_path = Arc::new(db_path);
    let mut handles = vec![];

    // Counter to track successful operations
    let success_counter = Arc::new(Mutex::new(0));

    // 5 reader threads
    for i in 0..5 {
        let db_path = Arc::clone(&db_path);
        let counter = Arc::clone(&success_counter);

        let handle = thread::spawn(move || {
            let conn = open_test_connection(db_path.as_path()).expect("Failed to open connection");

            for _ in 0..20 {
                // Try to read - might get 0 or more symbols depending on timing
                let _count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
                    .expect(&format!("Reader thread {} failed", i));

                // Increment success counter
                let mut count = counter.lock().unwrap();
                *count += 1;
            }
        });

        handles.push(handle);
    }

    // 3 writer threads (writing to the same database)
    for i in 0..3 {
        let db_path = Arc::clone(&db_path);
        let counter = Arc::clone(&success_counter);

        let handle = thread::spawn(move || {
            for j in 0..10 {
                // Use proper helper for connection
                let conn = open_test_connection(db_path.as_path())
                    .expect(&format!("Writer thread {} failed to open", i));

                // Insert a symbol (might conflict, but shouldn't corrupt)
                let result = conn.execute(
                    "INSERT OR REPLACE INTO symbols (
                        id, name, kind, file_path, start_line, end_line,
                        start_column, end_column, start_byte, end_byte,
                        signature, language
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    rusqlite::params![
                        format!("sym_{}_{}", i, j),
                        format!("Function{}", j),
                        "function",
                        format!("test_{}.rs", i),
                        1,
                        10,
                        0,
                        1,
                        0,
                        100,
                        "fn test()",
                        "rust"
                    ],
                );

                if result.is_ok() {
                    let mut count = counter.lock().unwrap();
                    *count += 1;
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let final_count = *success_counter.lock().unwrap();
    println!(
        "✅ Concurrent mixed access stress test passed: {} successful operations",
        final_count
    );

    // Verify database is not corrupted - can still query
    let conn = open_test_connection(db_path.as_path()).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .expect("Database corrupted - cannot query after concurrent access");

    println!(
        "✅ Database integrity verified: {} symbols after concurrent access",
        count
    );

    // Note: Count might be 0 if all writes conflicted or were in uncommitted transactions
    // The important thing is that the database didn't corrupt and we can still query it
    // This test validates that concurrent access doesn't cause "database malformed" errors
}

#[test]
#[ignore] // Long-running stress test - run manually
fn test_extreme_concurrent_stress() {
    use crate::tests::test_helpers::open_test_connection;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create database
    {
        let db = SymbolDatabase::new(&db_path).unwrap();
        drop(db);
    }

    let db_path = Arc::new(db_path);
    let mut handles = vec![];

    // 20 threads hammering the database for 10 seconds
    for i in 0..20 {
        let db_path = Arc::clone(&db_path);

        let handle = thread::spawn(move || {
            let start = std::time::Instant::now();
            let mut operations = 0;

            while start.elapsed() < Duration::from_secs(10) {
                let conn = open_test_connection(db_path.as_path())
                    .expect(&format!("Thread {} failed to open", i));

                // Mix of operations
                if i % 2 == 0 {
                    // Reader
                    let _: Result<i64, _> =
                        conn.query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0));
                } else {
                    // Writer
                    let _: Result<usize, _> = conn.execute(
                        "INSERT OR REPLACE INTO symbols (id, name, kind, file_path, start_line, end_line, start_column, end_column, start_byte, end_byte, signature, language) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                        rusqlite::params![
                            format!("extreme_{}_{}", i, operations),
                            "Test",
                            "function",
                            "test.rs",
                            1, 1, 0, 1, 0, 1,
                            "fn test()",
                            "rust"
                        ],
                    );
                }

                operations += 1;
            }

            println!("Thread {} completed {} operations", i, operations);
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked during stress test");
    }

    // Verify database integrity
    let conn = open_test_connection(db_path.as_path()).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .expect("Database corrupted after extreme stress test");

    println!(
        "✅ EXTREME stress test passed: {} symbols after 10 seconds of concurrent hammering",
        count
    );
}

/// ✅ GREEN TEST: Test WAL checkpoint functionality
#[test]
fn test_wal_checkpoint() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_checkpoint.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Database is created with WAL mode enabled, some initial writes have occurred
    // Now call checkpoint_wal() to merge WAL into main database
    let result = db.checkpoint_wal();

    assert!(result.is_ok(), "checkpoint_wal() should succeed");

    let (busy, log, checkpointed) = result.unwrap();

    // Verify checkpoint results
    // busy: Number of frames that couldn't be checkpointed (should be 0)
    // log: Total frames in WAL before checkpoint
    // checkpointed: Frames successfully checkpointed
    assert_eq!(busy, 0, "No frames should be busy during checkpoint");
    assert!(log >= 0, "Log should contain frames");
    assert!(checkpointed >= 0, "Should checkpoint frames");

    println!(
        "✅ WAL checkpoint successful: busy={}, log={}, checkpointed={}",
        busy, log, checkpointed
    );
}

/// Test that RESTART checkpoint mode waits for readers and successfully checkpoints
/// This prevents WAL files from growing to 45MB+ when PASSIVE checkpoints fail
#[test]
fn test_wal_checkpoint_restart_mode() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_checkpoint_restart.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create some test data to generate WAL activity
    // The database was just created, which generates WAL activity
    // Call checkpoint_wal_restart() which uses RESTART mode
    // RESTART waits for active readers to finish, then checkpoints
    let result = db.checkpoint_wal_restart();

    assert!(result.is_ok(), "checkpoint_wal_restart() should succeed");

    let (busy, log, checkpointed) = result.unwrap();

    // Verify checkpoint results
    assert_eq!(
        busy, 0,
        "RESTART mode should successfully checkpoint all frames"
    );
    assert!(log >= 0, "Log should contain frames");
    assert!(checkpointed >= 0, "Should checkpoint frames");

    println!(
        "✅ WAL checkpoint (RESTART) successful: busy={}, log={}, checkpointed={}",
        busy, log, checkpointed
    );
}

// 🚨 CRITICAL CORRUPTION PREVENTION TEST
// This test verifies the fix for "database disk image is malformed" errors
// Root cause: Connections were opened in DELETE mode, then WAL was set later
// Fix: WAL mode is now set IMMEDIATELY after connection open
#[test]
fn test_wal_mode_set_immediately_on_connection_open() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("wal_test.db");

    // Create database - this should set WAL mode immediately
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Verify WAL mode is active
    let journal_mode: String = db
        .conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();

    assert_eq!(
        journal_mode.to_lowercase(),
        "wal",
        "Database MUST be in WAL mode immediately after opening to prevent corruption"
    );

    // Verify synchronous mode is NORMAL (safe with WAL, faster than FULL)
    let sync_mode: i64 = db
        .conn
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();

    assert_eq!(
        sync_mode, 1,
        "Synchronous mode should be NORMAL (1) for performance with WAL"
    );

    println!(
        "✅ WAL mode verification passed: journal_mode={}, synchronous={}",
        journal_mode, sync_mode
    );
}

// 🚨 CORRUPTION PREVENTION: Test that Drop handler checkpoints WAL
#[test]
fn test_database_drop_checkpoints_wal() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("drop_test.db");

    {
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Write some data to create WAL entries
        db.store_file_with_content(
            "test.rs",
            "rust",
            "hash123",
            100,
            1234567890,
            "fn test() {}",
            "test_workspace",
        )
        .unwrap();

        // db goes out of scope here - Drop should checkpoint
    }

    // Reopen database - if Drop checkpoint worked, database should be clean
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Query should work without corruption
    let stats = db.get_stats().unwrap();
    assert_eq!(stats.total_files, 1);

    println!("✅ Drop checkpoint verified - database reopened cleanly");
}

// 🚨 SCHEMA VERSION SAFETY: Test that newer schema is detected
#[test]
fn test_schema_version_downgrade_detection() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("schema_test.db");

    // Create database with current schema
    {
        let db = SymbolDatabase::new(&db_path).unwrap();
        let version = db.get_schema_version().unwrap();
        assert_eq!(version, crate::database::LATEST_SCHEMA_VERSION);
    }

    // Manually bump schema version to simulate newer database
    {
        use rusqlite::Connection;
        let conn = Connection::open(&db_path).unwrap();
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
            .unwrap();

        let future_version = crate::database::LATEST_SCHEMA_VERSION + 10;
        conn.execute(
            "INSERT OR REPLACE INTO schema_version (version, applied_at, description)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![future_version, 1234567890, "Future schema"],
        )
        .unwrap();
    }

    // Try to open with old code - should fail with clear error
    let result = SymbolDatabase::new(&db_path);
    assert!(
        result.is_err(),
        "Should fail when database schema is newer than code expects"
    );

    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("NEWER than code expects"),
            "Error should explain schema version mismatch clearly. Got: {}",
            error_msg
        );
        println!("✅ Schema version downgrade detection working");
    }
}

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

// ============================================================
// MIGRATION 009 & REFERENCE SCORE TESTS
// ============================================================

/// Task 1: Verify migration 009 adds reference_score column with DEFAULT 0.0
#[test]
fn test_migration_009_reference_score_column_exists() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify reference_score column exists in symbols table
    let has_col = db.has_column("symbols", "reference_score").unwrap();
    assert!(
        has_col,
        "symbols table should have reference_score column after migration 009"
    );
}

/// Task 1: Verify reference_score defaults to 0.0 for newly inserted symbols
#[test]
fn test_reference_score_defaults_to_zero() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Insert a file (foreign key requirement)
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

    // Insert a symbol via raw SQL (to avoid any future default-setting logic)
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('sym1', 'test_fn', 'function', 'rust', 'test.rs', 1, 10, 0, 1, 0, 100)",
            [],
        )
        .unwrap();

    // Verify reference_score defaults to 0.0
    let score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'sym1'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(
        (score - 0.0).abs() < f64::EPSILON,
        "reference_score should default to 0.0, got {}",
        score
    );
}

/// Task 2: Verify compute_reference_scores applies correct weights
#[test]
fn test_compute_reference_scores_weighted() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

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
    for (id, name) in [
        ("target", "TargetFn"),
        ("caller1", "Caller1"),
        ("caller2", "Caller2"),
        ("caller3", "Caller3"),
    ] {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
                 VALUES (?1, ?2, 'function', 'rust', 'test.rs', 1, 10, 0, 1, 0, 100)",
                rusqlite::params![id, name],
            )
            .unwrap();
    }

    // Insert relationships TO target with different kinds:
    // caller1 --calls--> target (weight 3)
    // caller2 --imports--> target (weight 2)
    // caller3 --uses--> target (weight 1)
    for (rel_id, from_id, kind) in [
        ("r1", "caller1", "calls"),
        ("r2", "caller2", "imports"),
        ("r3", "caller3", "uses"),
    ] {
        db.conn
            .execute(
                "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
                 VALUES (?1, ?2, 'target', ?3)",
                rusqlite::params![rel_id, from_id, kind],
            )
            .unwrap();
    }

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
    let db = SymbolDatabase::new(&db_path).unwrap();

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
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('recursive_fn', 'factorial', 'function', 'rust', 'test.rs', 1, 10, 0, 1, 0, 100)",
            [],
        )
        .unwrap();

    // Insert self-referencing relationship (recursion)
    db.conn
        .execute(
            "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
             VALUES ('r_self', 'recursive_fn', 'recursive_fn', 'calls')",
            [],
        )
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
    let db = SymbolDatabase::new(&db_path).unwrap();

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

    // Insert symbols with known reference_scores via raw SQL
    for (id, name, score) in [
        ("s1", "fn_a", 5.0),
        ("s2", "fn_b", 0.0),
        ("s3", "fn_c", 12.5),
        ("s4", "fn_d", 3.0),
    ] {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte, reference_score)
                 VALUES (?1, ?2, 'function', 'rust', 'test.rs', 1, 10, 0, 1, 0, 100, ?3)",
                rusqlite::params![id, name, score],
            )
            .unwrap();
    }

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
    let db = SymbolDatabase::new(&db_path).unwrap();

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
    for (id, name) in [("sender", "send_data"), ("receiver", "receive_data")] {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
                 VALUES (?1, ?2, 'function', 'rust', 'test.rs', 1, 10, 0, 1, 0, 100)",
                rusqlite::params![id, name],
            )
            .unwrap();
    }

    // sender --calls--> receiver (sender has outgoing, no incoming)
    db.conn
        .execute(
            "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
             VALUES ('r1', 'sender', 'receiver', 'calls')",
            [],
        )
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

/// C# DI pattern: interface gets all the centrality, concrete implementation gets zero.
/// After propagation, the implementing class should inherit a fraction of the interface's score.
#[test]
fn test_compute_reference_scores_propagates_interface_centrality_to_implementations() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(&FileInfo {
        path: "test.cs".to_string(),
        language: "csharp".to_string(),
        hash: "abc123".to_string(),
        size: 1000,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 5,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // IService (interface), ServiceImpl (class), 3 consumers
    for (id, name, kind) in [
        ("iservice", "IService", "interface"),
        ("service_impl", "ServiceImpl", "class"),
        ("consumer1", "Consumer1", "class"),
        ("consumer2", "Consumer2", "class"),
        ("consumer3", "Consumer3", "class"),
    ] {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
                 VALUES (?1, ?2, ?3, 'csharp', 'test.cs', 1, 10, 0, 1, 0, 100)",
                rusqlite::params![id, name, kind],
            )
            .unwrap();
    }

    // ServiceImpl implements IService
    db.conn
        .execute(
            "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
             VALUES ('r_impl', 'service_impl', 'iservice', 'implements')",
            [],
        )
        .unwrap();

    // 3 consumers reference IService (constructor params / field types)
    for (rel_id, from_id) in [
        ("r1", "consumer1"),
        ("r2", "consumer2"),
        ("r3", "consumer3"),
    ] {
        db.conn
            .execute(
                "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
                 VALUES (?1, ?2, 'iservice', 'uses')",
                rusqlite::params![rel_id, from_id],
            )
            .unwrap();
    }

    db.compute_reference_scores().unwrap();

    // IService: 3 × uses(1) + 1 × implements(2) = 5.0
    let iservice_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'iservice'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (iservice_score - 5.0).abs() < f64::EPSILON,
        "IService should have ref_score 5.0, got {}",
        iservice_score
    );

    // ServiceImpl should inherit centrality from IService via implements relationship
    let impl_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'service_impl'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        impl_score > 0.0,
        "ServiceImpl should inherit centrality from IService via implements, got {}",
        impl_score
    );
    // Should be a meaningful fraction of the interface's score
    assert!(
        impl_score >= iservice_score * 0.5,
        "ServiceImpl should get at least 50% of IService's score ({}), got {}",
        iservice_score,
        impl_score
    );
}

/// Same propagation should work for extends (class inheritance)
#[test]
fn test_compute_reference_scores_propagates_base_class_centrality() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(&FileInfo {
        path: "test.cs".to_string(),
        language: "csharp".to_string(),
        hash: "abc123".to_string(),
        size: 1000,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 4,
        line_count: 0,
        content: None,
    })
    .unwrap();

    for (id, name, kind) in [
        ("base_class", "BaseService", "class"),
        ("derived", "DerivedService", "class"),
        ("caller1", "Caller1", "class"),
        ("caller2", "Caller2", "class"),
    ] {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
                 VALUES (?1, ?2, ?3, 'csharp', 'test.cs', 1, 10, 0, 1, 0, 100)",
                rusqlite::params![id, name, kind],
            )
            .unwrap();
    }

    // DerivedService extends BaseService
    db.conn
        .execute(
            "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
             VALUES ('r_ext', 'derived', 'base_class', 'extends')",
            [],
        )
        .unwrap();

    // 2 callers reference BaseService
    for (rel_id, from_id) in [("r1", "caller1"), ("r2", "caller2")] {
        db.conn
            .execute(
                "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
                 VALUES (?1, ?2, 'base_class', 'calls')",
                rusqlite::params![rel_id, from_id],
            )
            .unwrap();
    }

    db.compute_reference_scores().unwrap();

    // BaseService: 2 × calls(3) + 1 × extends(2) = 8.0
    let base_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'base_class'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (base_score - 8.0).abs() < f64::EPSILON,
        "BaseService should have ref_score 8.0, got {}",
        base_score
    );

    // DerivedService should inherit some centrality
    let derived_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'derived'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        derived_score > 0.0,
        "DerivedService should inherit centrality from BaseService via extends, got {}",
        derived_score
    );
}

#[test]
fn test_delete_embeddings_for_symbol_ids_only_removes_requested_rows() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert one file and three symbols for embedding rows.
    db.conn
        .execute(
            "INSERT INTO files (path, language, hash, size, last_modified, last_indexed)
             VALUES ('src/lib.rs', 'rust', 'hash123', 100, 0, 0)",
            [],
        )
        .unwrap();

    for (id, name) in [("sym_a", "a"), ("sym_b", "b"), ("sym_c", "c")] {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
                 VALUES (?1, ?2, 'function', 'rust', 'src/lib.rs', 1, 1, 0, 1, 0, 1)",
                rusqlite::params![id, name],
            )
            .unwrap();
    }

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

/// Verify get_reference_scores batches correctly with >900 IDs (SQLite bind param limit).
#[test]
fn test_get_reference_scores_large_batch() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

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
    for i in 0..count {
        let id = format!("sym_{i}");
        let score = (i % 50) as f64;
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, \
                 start_line, end_line, start_col, end_col, start_byte, end_byte, \
                 reference_score) \
                 VALUES (?1, ?2, 'function', 'rust', 'test.rs', 1, 10, 0, 1, 0, 100, ?3)",
                rusqlite::params![id, format!("fn_{i}"), score],
            )
            .unwrap();
    }

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

// ============================================================================
// Migration 011: Embedding Config (Phase 5, Task 1)
// ============================================================================

#[test]
fn test_migration_011_creates_embedding_config_table() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Verify embedding_config table exists
    let table_exists: bool = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='embedding_config'",
            [],
            |row| row.get::<_, i32>(0).map(|c| c > 0),
        )
        .unwrap();
    assert!(
        table_exists,
        "embedding_config table should exist after migration 011"
    );
}

#[test]
fn test_migration_011_is_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create database (runs all migrations including 011)
    {
        let _db = SymbolDatabase::new(&db_path).unwrap();
    }

    // Re-open — should not error
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();
    let version = db.get_schema_version().unwrap();
    assert_eq!(version, LATEST_SCHEMA_VERSION);

    // Config should still have defaults
    let (model, dims) = db.get_embedding_config().unwrap();
    assert_eq!(model, "bge-small-en-v1.5");
    assert_eq!(dims, 384);
}

/// Task 6: Constructor centrality propagation to parent class.
/// In C# / Java / TypeScript with DI, all references target the constructor,
/// leaving the class itself with zero centrality.
#[test]
fn test_compute_reference_scores_propagates_constructor_centrality() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(&FileInfo {
        path: "src/services.cs".to_string(),
        language: "csharp".to_string(),
        hash: "abc123".to_string(),
        size: 1000,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 5,
        line_count: 0,
        content: None,
    })
    .unwrap();

    db.store_file_info(&FileInfo {
        path: "src/program.cs".to_string(),
        language: "csharp".to_string(),
        hash: "def456".to_string(),
        size: 500,
        last_modified: 1234567890,
        last_indexed: 0,
        symbol_count: 2,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Class with no direct references
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte, visibility)
             VALUES ('class_1', 'LabTestService', 'class', 'csharp', 'src/services.cs', 1, 100, 0, 1, 0, 5000, 'public')",
            [],
        )
        .unwrap();

    // Constructor with parent_id pointing to the class
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte, visibility, parent_id)
             VALUES ('ctor_1', 'LabTestService', 'constructor', 'csharp', 'src/services.cs', 10, 15, 0, 1, 200, 400, 'public', 'class_1')",
            [],
        )
        .unwrap();

    // Two callers that reference the constructor (DI registrations)
    for (id, name) in [("caller_1", "ConfigureServices"), ("caller_2", "TestSetup")] {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte, visibility)
                 VALUES (?1, ?2, 'method', 'csharp', 'src/program.cs', 50, 80, 0, 1, 0, 500, 'public')",
                rusqlite::params![id, name],
            )
            .unwrap();
    }

    // Relationships: callers -> constructor (DI pattern)
    db.conn
        .execute(
            "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
             VALUES ('rel_1', 'caller_1', 'ctor_1', 'instantiates')",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
             VALUES ('rel_2', 'caller_2', 'ctor_1', 'uses')",
            [],
        )
        .unwrap();

    db.compute_reference_scores().unwrap();

    // Constructor should have centrality from DI references:
    // instantiates=2 + uses=1 = 3.0
    let ctor_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'ctor_1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        ctor_score > 0.0,
        "Constructor should have centrality from DI references, got {}",
        ctor_score
    );

    // Class should inherit constructor centrality
    let class_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'class_1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        class_score > 0.0,
        "Class should inherit constructor centrality, got {}",
        class_score
    );
    assert!(
        class_score >= ctor_score * 0.5,
        "Class should get at least 50% of constructor centrality (ctor={}), got {}",
        ctor_score,
        class_score
    );
}

/// Verify that TypeUsage identifiers contribute to centrality even without relationships.
/// This fixes the GDScript pattern where classes are referenced via type annotations
/// (var x: PandoraEntity, func f() -> PandoraEntity) but no call relationships exist.
#[test]
fn test_compute_reference_scores_includes_type_usage_identifiers() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Insert two files
    for (path, lang) in [
        ("model/entity.gd", "gdscript"),
        ("backend/api.gd", "gdscript"),
    ] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: lang.to_string(),
            hash: "abc123".to_string(),
            size: 100,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 2,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // Insert class symbol: PandoraEntity in entity.gd
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('entity_class', 'PandoraEntity', 'class', 'gdscript', 'model/entity.gd', 1, 100, 0, 1, 0, 500)",
            [],
        )
        .unwrap();

    // Insert a function that references PandoraEntity via type annotation
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('api_func', 'create_entity', 'function', 'gdscript', 'backend/api.gd', 10, 20, 0, 1, 0, 200)",
            [],
        )
        .unwrap();

    // NO relationships — this is the key. Only identifiers.
    // Insert TypeUsage identifiers pointing to PandoraEntity by name from api.gd
    for (id, line) in [("id1", 10), ("id2", 15), ("id3", 18)] {
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id)
                 VALUES (?1, 'PandoraEntity', 'type_usage', 'gdscript', 'backend/api.gd', ?2, 0, ?2, 15, 'api_func')",
                rusqlite::params![id, line],
            )
            .unwrap();
    }

    // Compute scores
    db.compute_reference_scores().unwrap();

    // PandoraEntity should now have non-zero centrality from type usage identifiers
    let entity_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'entity_class'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        entity_score > 0.0,
        "PandoraEntity should have non-zero centrality from TypeUsage identifiers, got {}",
        entity_score
    );

    // create_entity function has no incoming refs — should stay at 0
    let func_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'api_func'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (func_score - 0.0).abs() < f64::EPSILON,
        "create_entity should have 0.0 centrality (no incoming refs), got {}",
        func_score
    );
}

/// Verify that Zig-style type constants (const Server = @This()) get centrality from TypeUsage.
/// In Zig, types are `constant` kind, not `class`/`struct`.
#[test]
fn test_compute_reference_scores_includes_constants_with_type_usage() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    for (path, lang) in [("src/Server.zig", "zig"), ("src/main.zig", "zig")] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: lang.to_string(),
            hash: "abc123".to_string(),
            size: 100,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 2,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // Zig type constant: const Server = @This()
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('server_const', 'Server', 'constant', 'zig', 'src/Server.zig', 1, 1, 0, 1, 0, 30)",
            [],
        )
        .unwrap();

    // A plain constant that's NOT used as a type (should NOT get boosted)
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('max_const', 'max_retries', 'constant', 'zig', 'src/Server.zig', 5, 5, 0, 1, 0, 30)",
            [],
        )
        .unwrap();

    // TypeUsage identifiers referencing Server from main.zig
    for (id, line) in [("id1", 10), ("id2", 20), ("id3", 30)] {
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col)
                 VALUES (?1, 'Server', 'type_usage', 'zig', 'src/main.zig', ?2, 0, ?2, 10)",
                rusqlite::params![id, line],
            )
            .unwrap();
    }

    db.compute_reference_scores().unwrap();

    // Server constant should get centrality from TypeUsage identifiers
    let server_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'server_const'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        server_score > 0.0,
        "Zig type constant 'Server' should have non-zero centrality from TypeUsage, got {}",
        server_score
    );

    // max_retries has no TypeUsage identifiers — should stay at 0
    let max_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'max_const'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (max_score - 0.0).abs() < f64::EPSILON,
        "Plain constant 'max_retries' should have 0.0 centrality (no TypeUsage refs), got {}",
        max_score
    );
}

/// Verify that import identifiers contribute to centrality.
/// In Zig, cross-file references are primarily @import() which produce import-kind identifiers.
/// A symbol imported in 15 files should have significant centrality.
#[test]
fn test_compute_reference_scores_includes_import_identifiers() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Create files
    for path in ["src/Store.zig", "src/a.zig", "src/b.zig", "src/c.zig"] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: "zig".to_string(),
            hash: "abc123".to_string(),
            size: 100,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // The type constant being imported
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('store', 'DocumentStore', 'constant', 'zig', 'src/Store.zig', 1, 1, 0, 1, 0, 30)",
            [],
        )
        .unwrap();

    // Import identifiers from 3 different files (weight 2.0 each)
    for (id, file) in [
        ("imp1", "src/a.zig"),
        ("imp2", "src/b.zig"),
        ("imp3", "src/c.zig"),
    ] {
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col)
                 VALUES (?1, 'DocumentStore', 'import', 'zig', ?2, 1, 0, 1, 30)",
                rusqlite::params![id, file],
            )
            .unwrap();
    }

    db.compute_reference_scores().unwrap();

    let score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'store'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // 3 imports × 2.0 weight = 6.0
    assert!(
        (score - 6.0).abs() < f64::EPSILON,
        "DocumentStore should have centrality 6.0 from 3 imports (3 × 2.0), got {}",
        score
    );
}

/// Verify that qualified-name identifiers (e.g. Kirigami.ScrollablePage) match
/// unqualified symbol names (ScrollablePage) for centrality computation.
/// QML uses namespace-qualified references heavily: `Kirigami.ScrollablePage {}`.
/// Without this, all QML components have centrality 0.00 despite heavy usage.
#[test]
fn test_compute_reference_scores_qualified_name_identifiers() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    for (path, lang) in [
        ("src/controls/ScrollablePage.qml", "qml"),
        ("src/controls/AboutPage.qml", "qml"),
        ("examples/SimplePage.qml", "qml"),
    ] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: lang.to_string(),
            hash: "abc123".to_string(),
            size: 100,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 2,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // QML component: file-derived name "ScrollablePage"
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('scrollable_page', 'ScrollablePage', 'class', 'qml', 'src/controls/ScrollablePage.qml', 1, 100, 0, 1, 0, 2000)",
            [],
        )
        .unwrap();

    // Another component that should NOT be matched
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('about_page', 'AboutPage', 'class', 'qml', 'src/controls/AboutPage.qml', 1, 50, 0, 1, 0, 1000)",
            [],
        )
        .unwrap();

    // TypeUsage identifiers using QUALIFIED name: "Kirigami.ScrollablePage"
    // These reference ScrollablePage but through a namespace prefix
    for (id, line) in [("id1", 10), ("id2", 20), ("id3", 30)] {
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col)
                 VALUES (?1, 'Kirigami.ScrollablePage', 'type_usage', 'qml', 'examples/SimplePage.qml', ?2, 0, ?2, 30)",
                rusqlite::params![id, line],
            )
            .unwrap();
    }

    db.compute_reference_scores().unwrap();

    // ScrollablePage should have non-zero centrality from qualified type usage identifiers
    let score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'scrollable_page'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // 3 type_usage × 1.0 weight = 3.0
    assert!(
        (score - 3.0).abs() < f64::EPSILON,
        "ScrollablePage should have centrality 3.0 from 3 qualified TypeUsage refs (Kirigami.ScrollablePage), got {}",
        score
    );

    // AboutPage should NOT be boosted — no identifiers reference it
    let about_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'about_page'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        (about_score - 0.0).abs() < f64::EPSILON,
        "AboutPage should have 0.0 centrality (no matching identifiers), got {}",
        about_score
    );
}

#[test]
fn test_compute_reference_scores_escapes_like_wildcards() {
    // Symbol names with underscores must NOT act as SQL LIKE wildcards.
    // `_` in LIKE matches any single character, so `user_id` would match
    // `userXid` without escaping. This test verifies proper escaping.
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    for path in ["src/models.py", "src/views.py"] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: "python".to_string(),
            hash: "abc123".to_string(),
            size: 100,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 2,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // Symbol with underscore: user_id
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('sym_user_id', 'user_id', 'class', 'python', 'src/models.py', 1, 10, 0, 1, 0, 200)",
            [],
        )
        .unwrap();

    // Symbol WITHOUT underscore that would match if _ is a wildcard: userXid
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('sym_userXid', 'userXid', 'class', 'python', 'src/models.py', 20, 30, 0, 1, 0, 200)",
            [],
        )
        .unwrap();

    // Qualified identifier referencing "models.userXid"
    // This should match userXid (exact suffix) but NOT user_id (would only
    // match if underscore acts as wildcard)
    db.conn
        .execute(
            "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col)
             VALUES ('ref1', 'models.userXid', 'type_usage', 'python', 'src/views.py', 5, 0, 5, 20)",
            [],
        )
        .unwrap();

    db.compute_reference_scores().unwrap();

    let user_id_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'sym_user_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    let userxid_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'sym_userXid'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // user_id must NOT be boosted — "models.userXid" should not match "user_id"
    assert!(
        (user_id_score - 0.0).abs() < f64::EPSILON,
        "user_id must NOT match 'models.userXid' via underscore wildcard. Got score: {}",
        user_id_score
    );

    // userXid SHOULD be boosted — "models.userXid" matches via suffix
    assert!(
        userxid_score > 0.0,
        "userXid should be boosted by 'models.userXid' qualified ref. Got score: {}",
        userxid_score
    );
}

/// Test that symbols in test files get de-weighted during centrality computation.
/// Reproduces the Flask problem: tests/test_config.py defines `class Flask(flask.Flask)`
/// which steals centrality from the real `src/flask/app.py::Flask` because both share
/// the name and the test subclass accumulates references from test files.
#[test]
fn test_centrality_deweights_test_file_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Real source file
    db.store_file_info(&FileInfo {
        path: "src/flask/app.py".to_string(),
        language: "python".to_string(),
        hash: "real".to_string(),
        size: 5000,
        last_modified: 1,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Test file that defines a Flask subclass
    db.store_file_info(&FileInfo {
        path: "tests/test_config.py".to_string(),
        language: "python".to_string(),
        hash: "test".to_string(),
        size: 1000,
        last_modified: 1,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Additional test files that reference Flask
    for i in 0..5 {
        db.store_file_info(&FileInfo {
            path: format!("tests/test_app_{}.py", i),
            language: "python".to_string(),
            hash: format!("testhash{}", i),
            size: 500,
            last_modified: 1,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // A real source file that references Flask
    db.store_file_info(&FileInfo {
        path: "src/flask/views.py".to_string(),
        language: "python".to_string(),
        hash: "views".to_string(),
        size: 2000,
        last_modified: 1,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    })
    .unwrap();

    // Real Flask class in source
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('real_flask', 'Flask', 'class', 'python', 'src/flask/app.py', 1, 500, 0, 1, 0, 10000)",
            [],
        )
        .unwrap();

    // Test Flask subclass in test file
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('test_flask', 'Flask', 'class', 'python', 'tests/test_config.py', 1, 50, 0, 1, 0, 1000)",
            [],
        )
        .unwrap();

    // Caller symbols in test files
    for i in 0..5 {
        db.conn
            .execute(
                &format!(
                    "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
                     VALUES ('test_caller_{}', 'test_func_{}', 'function', 'python', 'tests/test_app_{}.py', 1, 10, 0, 1, 0, 100)",
                    i, i, i
                ),
                [],
            )
            .unwrap();
    }

    // Caller in real source file
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('real_caller', 'create_app', 'function', 'python', 'src/flask/views.py', 1, 10, 0, 1, 0, 200)",
            [],
        )
        .unwrap();

    // 5 test files reference the test Flask (instantiates, weight=2 each => 10 total)
    for i in 0..5 {
        db.conn
            .execute(
                &format!(
                    "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
                     VALUES ('r_test_{}', 'test_caller_{}', 'test_flask', 'instantiates')",
                    i, i
                ),
                [],
            )
            .unwrap();
    }

    // 1 real source file references real Flask (instantiates, weight=2)
    db.conn
        .execute(
            "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
             VALUES ('r_real', 'real_caller', 'real_flask', 'instantiates')",
            [],
        )
        .unwrap();

    db.compute_reference_scores().unwrap();

    let real_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'real_flask'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let test_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'test_flask'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // Real Flask should have HIGHER centrality than test Flask
    assert!(
        real_score > test_score,
        "Real Flask (score={}) should have higher centrality than test Flask (score={}). \
         Test-file symbols should be de-weighted.",
        real_score,
        test_score
    );

    // Test Flask score should be significantly reduced (at most 50% of raw score)
    assert!(
        test_score < 5.0,
        "Test Flask score ({}) should be significantly reduced from raw 10.0",
        test_score
    );
}

/// C/C++ header/implementation centrality split: header declarations accumulate all
/// reference_score (via #include) while implementations in .c/.cpp get zero.
/// Step 5 propagates 70% of header centrality to same-named implementations.
#[test]
fn test_compute_reference_scores_propagates_header_to_implementation() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    for (path, lang) in [
        ("jq.h", "c"),
        ("execute.c", "c"),
        ("main.c", "c"),
        ("parser.c", "c"),
    ] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: lang.to_string(),
            hash: "abc123".to_string(),
            size: 1000,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 5,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // Header declaration: jq_next in jq.h
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('jq_next_h', 'jq_next', 'function', 'c', 'jq.h', 10, 10, 0, 30, 100, 130)",
            [],
        )
        .unwrap();

    // Implementation: jq_next in execute.c
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('jq_next_c', 'jq_next', 'function', 'c', 'execute.c', 50, 120, 0, 1, 500, 2500)",
            [],
        )
        .unwrap();

    // Callers that reference the header declaration
    for (id, name, file) in [
        ("caller_main", "main", "main.c"),
        ("caller_parse", "parse_input", "parser.c"),
        ("caller_run", "run_program", "main.c"),
    ] {
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
                 VALUES (?1, ?2, 'function', 'c', ?3, 1, 20, 0, 1, 0, 300)",
                rusqlite::params![id, name, file],
            )
            .unwrap();
    }

    // Relationships: callers -> header declaration
    for (rel_id, from_id, kind) in [
        ("rel_1", "caller_main", "calls"),
        ("rel_2", "caller_parse", "calls"),
        ("rel_3", "caller_run", "uses"),
    ] {
        db.conn
            .execute(
                "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind)
                 VALUES (?1, ?2, 'jq_next_h', ?3)",
                rusqlite::params![rel_id, from_id, kind],
            )
            .unwrap();
    }

    db.compute_reference_scores().unwrap();

    let header_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'jq_next_h'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        header_score > 0.0,
        "Header declaration should have centrality from callers, got {}",
        header_score
    );

    let impl_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'jq_next_c'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        impl_score > 0.0,
        "Implementation should get propagated centrality from header, got {}",
        impl_score
    );

    let expected_impl_score = header_score * 0.7;
    assert!(
        (impl_score - expected_impl_score).abs() < 0.01,
        "Implementation should get exactly 70% of header score (header={}, expected={}, got={})",
        header_score,
        expected_impl_score,
        impl_score
    );
}

/// Step 1b excludes test-file symbols from the name-based identifier boost.
/// Two class symbols named "Flask" — one in production, one in tests — should
/// receive very different centrality when cross-file type_usage identifiers exist.
#[test]
fn test_step1b_identifier_boost_excludes_test_file_symbols() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    for (path, lang) in [
        ("src/app.py", "python"),
        ("tests/test_config.py", "python"),
        ("src/routes.py", "python"),
        ("src/views.py", "python"),
        ("src/auth.py", "python"),
        ("src/models.py", "python"),
        ("src/forms.py", "python"),
        ("src/cli.py", "python"),
        ("src/utils.py", "python"),
        ("src/admin.py", "python"),
        ("src/api.py", "python"),
        ("src/middleware.py", "python"),
    ] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: lang.to_string(),
            hash: "abc".to_string(),
            size: 100,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 1,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // Production Flask class
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('flask_prod', 'Flask', 'class', 'python', 'src/app.py', 109, 500, 0, 1, 0, 10000)",
            [],
        )
        .unwrap();

    // Test Flask class
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, language, file_path, start_line, end_line, start_col, end_col, start_byte, end_byte)
             VALUES ('flask_test', 'Flask', 'class', 'python', 'tests/test_config.py', 202, 250, 0, 1, 0, 5000)",
            [],
        )
        .unwrap();

    // Add type_usage identifiers named "Flask" from 10 non-test files
    let source_files = [
        "src/routes.py",
        "src/views.py",
        "src/auth.py",
        "src/models.py",
        "src/forms.py",
        "src/cli.py",
        "src/utils.py",
        "src/admin.py",
        "src/api.py",
        "src/middleware.py",
    ];
    for (i, file) in source_files.iter().enumerate() {
        db.conn
            .execute(
                "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col)
                 VALUES (?1, 'Flask', 'type_usage', 'python', ?2, 1, 0, 1, 10)",
                rusqlite::params![format!("id_type_{}", i), file],
            )
            .unwrap();
    }

    db.compute_reference_scores().unwrap();

    let prod_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'flask_prod'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let test_score: f64 = db
        .conn
        .query_row(
            "SELECT reference_score FROM symbols WHERE id = 'flask_test'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(
        prod_score > 0.0,
        "Production Flask should receive Step 1b identifier boost, got {}",
        prod_score
    );
    assert!(
        (test_score - 0.0).abs() < f64::EPSILON,
        "Test-file Flask should NOT receive Step 1b identifier boost, got {}",
        test_score
    );
}
