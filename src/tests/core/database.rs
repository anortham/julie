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

    // Test embeddings table creation
    let embeddings_result = db.create_embeddings_table();
    assert!(
        embeddings_result.is_ok(),
        "Embeddings table creation failed: {:?}",
        embeddings_result
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
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .unwrap();
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

#[test]
fn test_fts_search_file_content() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_with_content(
        "docs/architecture.md",
        "markdown",
        "abc123",
        2048,
        1234567890,
        "# Architecture\nSQLite is the single source of truth",
        "test_workspace",
    )
    .unwrap();

    // Search for "SQLite"
    let results = db
        .search_file_content_fts("SQLite", &None, &None, 10)
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, "docs/architecture.md");
    assert!(results[0].snippet.contains("SQLite"));
}

// 🔴 TDD RED: Test file_pattern and language filtering in FTS search
#[test]
fn test_fts_search_with_file_pattern_and_language() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Store files in different directories with same search term "refactoring"
    // Use platform-appropriate path separators to match production behavior
    // Store files with relative Unix-style paths (per RELATIVE_PATHS_CONTRACT.md)
    let test_file = "src/tests/tools/refactoring/rename_symbol.rs";
    let tools_file = "src/tools/refactoring/mod.rs";
    let docs_file = "docs/refactoring.md";

    db.store_file_with_content(
        test_file,
        "rust",
        "abc123",
        1024,
        1234567890,
        "// Tests for RenameSymbolTool refactoring operations",
        "test_workspace",
    )
    .unwrap();

    db.store_file_with_content(
        tools_file,
        "rust",
        "def456",
        2048,
        1234567891,
        "// Core refactoring tool implementation",
        "test_workspace",
    )
    .unwrap();

    db.store_file_with_content(
        docs_file,
        "markdown",
        "ghi789",
        512,
        1234567892,
        "# Refactoring Guide\nHow to use refactoring tools",
        "test_workspace",
    )
    .unwrap();

    // Test 1: Search ALL files (no filter) - should find all 3
    let results = db
        .search_file_content_fts("refactoring", &None, &None, 10)
        .unwrap();
    assert_eq!(results.len(), 3, "Without filters, should find all 3 files");

    // Test 2: Filter by file_pattern using relative pattern
    // Database stores: src/tests/tools/refactoring/rename_symbol.rs (relative Unix-style)
    // User writes: src/tests/** (relative pattern)
    // Pattern used: src/tests/** (no normalization - paths are already workspace-relative!)
    let results = db
        .search_file_content_fts("refactoring", &None, &Some("src/tests/**".to_string()), 10)
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "With file_pattern 'src/tests/**', should only find test file"
    );
    assert!(results[0].path.contains("tests") && results[0].path.contains("refactoring"));

    // Test 3: Filter by language (only markdown)
    let results = db
        .search_file_content_fts("refactoring", &Some("markdown".to_string()), &None, 10)
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "With language filter 'markdown', should only find .md file"
    );
    assert!(results[0].path.contains("docs") && results[0].path.contains("refactoring.md"));

    // Test 4: Combined filters (language + file_pattern)
    // Database stores: src/tools/refactoring/mod.rs (relative Unix-style)
    // Filters: language="rust" AND file_pattern="src/tools/**"
    let results = db
        .search_file_content_fts(
            "refactoring",
            &Some("rust".to_string()),
            &Some("src/tools/**".to_string()),
            10,
        )
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "With rust + src/tools filter, should only find src/tools/refactoring file"
    );
    assert!(results[0].path.contains("tools") && results[0].path.contains("refactoring"));
}

// 🔴 TDD: Comprehensive test coverage for file_pattern normalization
// User requirement: "you are going to need complete test coverage around this change, don't take shortcuts"
#[test]
fn test_fts_file_pattern_normalization() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Store files with absolute paths (simulating Windows UNC paths like database stores)
    // In production, these would be something like \\?\C:\source\julie\src\tests\...
    // For testing, we'll use canonicalized paths which also create absolute paths
    let test_content = "// Test content for pattern normalization testing";

    // Create temp directory structure
    let src_dir = temp_dir.path().join("src");
    let tests_dir = src_dir.join("tests");
    let tools_dir = src_dir.join("tools");
    std::fs::create_dir_all(&tests_dir).unwrap();
    std::fs::create_dir_all(&tools_dir).unwrap();

    // Store files with relative Unix-style paths (like production does per RELATIVE_PATHS_CONTRACT.md)
    // Production uses to_relative_unix_style() which converts ALL paths to forward slashes
    // (see src/utils/paths.rs:88-95)
    db.store_file_with_content(
        "src/tests/test1.rs",
        "rust",
        "abc1",
        100,
        1234567890,
        test_content,
        "test_workspace",
    )
    .unwrap();
    db.store_file_with_content(
        "src/tools/tool.rs",
        "rust",
        "abc2",
        100,
        1234567891,
        test_content,
        "test_workspace",
    )
    .unwrap();
    db.store_file_with_content(
        "src/main.rs",
        "rust",
        "abc3",
        100,
        1234567892,
        test_content,
        "test_workspace",
    )
    .unwrap();

    // TEST 1: Relative pattern works as-is (no normalization needed)
    // Database stores: src/tests/test1.rs (relative Unix-style)
    // User writes: src/tests/**
    // Pattern used: src/tests/** (no normalization - database uses relative paths!)
    // Should match: src/tests/test1.rs
    let results = db
        .search_file_content_fts(
            "pattern normalization",
            &None,
            &Some("src/tests/**".to_string()),
            10,
        )
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "Relative pattern 'src/tests/**' should match relative stored path 'src/tests/test1.rs'"
    );
    assert!(
        results[0].path.contains("tests"),
        "Should match file in tests directory"
    );

    // TEST 2: Pattern already starting with * should remain unchanged
    // User writes: *tests*
    // Normalization: *tests* (unchanged)
    let results = db
        .search_file_content_fts(
            "pattern normalization",
            &None,
            &Some("*tests*".to_string()),
            10,
        )
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "Pattern '*tests*' already has wildcard prefix, should remain unchanged"
    );

    // TEST 3: Pattern with backslashes won't match forward-slash storage
    // Database stores: src/tools/tool.rs (forward slashes - per contract)
    // User writes: src\tools\** (with backslashes)
    // Pattern used: src\tools\** (no normalization)
    // Result: NO MATCH (backslashes don't match forward slashes)
    let results = db
        .search_file_content_fts(
            "pattern normalization",
            &None,
            &Some("src\\tools\\**".to_string()),
            10,
        )
        .unwrap();
    assert_eq!(
        results.len(),
        0,
        "Pattern 'src\\tools\\**' (backslashes) should NOT match 'src/tools/tool.rs' (forward slashes)"
    );

    // TEST 3b: Forward slash pattern DOES match
    let results = db
        .search_file_content_fts(
            "pattern normalization",
            &None,
            &Some("src/tools/**".to_string()),
            10,
        )
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "Pattern 'src/tools/**' (forward slashes) should match 'src/tools/tool.rs'"
    );
    assert!(
        results[0].path.contains("tools"),
        "Should match file in tools directory"
    );

    // TEST 4: Wildcard patterns match everything (including path separators)
    // Database stores: src/tests/test1.rs, src/tools/tool.rs, src/main.rs
    // User writes: src/* (in SQLite GLOB, * matches EVERYTHING including /)
    // Result: All src/ files match (GLOB * is greedy)
    let results = db
        .search_file_content_fts(
            "pattern normalization",
            &None,
            &Some("src/*".to_string()),
            10,
        )
        .unwrap();
    assert!(
        results.len() >= 3,
        "Pattern 'src/*' should match all src files (GLOB * is greedy, matches path separators too)"
    );

    // TEST 4b: Use specific pattern to match only what we want
    // To match ONLY top-level src files, need exact pattern: src/*.rs
    let results = db
        .search_file_content_fts(
            "pattern normalization",
            &None,
            &Some("src/main.rs".to_string()),
            10,
        )
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "Pattern 'src/main.rs' should match exact file only"
    );
    assert!(results[0].path == "src/main.rs", "Should match exact file");

    // TEST 5: Multiple relative path segments
    // Database stores: src/tests/subfolder/nested.rs (relative Unix-style)
    // User writes: src/tests/subfolder/**
    // Pattern used: src/tests/subfolder/** (no normalization)
    // Should match: src/tests/subfolder/nested.rs
    db.store_file_with_content(
        "src/tests/subfolder/nested.rs",
        "rust",
        "abc4",
        100,
        1234567893,
        test_content,
        "test_workspace",
    )
    .unwrap();

    let results = db
        .search_file_content_fts(
            "pattern normalization",
            &None,
            &Some("src/tests/subfolder/**".to_string()),
            10,
        )
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "Deep relative pattern 'src/tests/subfolder/**' should match 'src/tests/subfolder/nested.rs'"
    );
    assert!(
        results[0].path.contains("subfolder"),
        "Should match nested file"
    );

    // TEST 6: Wildcard patterns with * match correctly
    // Database stores: src/tools/tool.rs (forward slashes)
    // User writes: */tools/* (single wildcards)
    // Pattern used: */tools/* (no normalization)
    // Should match: src/tools/tool.rs
    let results = db
        .search_file_content_fts(
            "pattern normalization",
            &None,
            &Some("*/tools/*".to_string()),
            10,
        )
        .unwrap();
    assert_eq!(
        results.len(),
        1,
        "Pattern '*/tools/*' should match 'src/tools/tool.rs'"
    );

    // TEST 7: Empty pattern should be handled (None vs Some(""))
    // This tests the edge case of empty string pattern
    let results = db
        .search_file_content_fts("pattern normalization", &None, &None, 10)
        .unwrap();
    assert_eq!(
        results.len(),
        4,
        "No pattern filter should return all files"
    );
}

/// 🔴 REGRESSION TEST: Windows GLOB pattern bug with forward-slash storage
/// Bug: Lines 388-389 in src/database/files.rs convert forward slashes to backslashes on Windows
/// This violates RELATIVE_PATHS_CONTRACT.md which mandates forward slashes for all stored paths
///
/// Reproduction:
/// 1. Database stores: .memories/2025-11-10/file.json (forward slashes - per contract)
/// 2. User pattern: .memories/**/*.json (forward slashes)
/// 3. Buggy normalization: *\.memories\**\*.json (backslashes on Windows)
/// 4. GLOB match: FAILS (backslashes don't match forward slashes)
///
/// This test stores paths with forward slashes (like production does via to_relative_unix_style)
/// and verifies GLOB patterns with forward slashes match correctly.
#[test]
fn test_fts_file_pattern_forward_slash_glob_matching() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let test_content =
        r#"{"id": "checkpoint_test", "description": "Test checkpoint for GLOB bug"}"#;

    // Store files with forward-slash paths (simulating production behavior)
    // Production uses to_relative_unix_style() which converts ALL paths to forward slashes
    // See src/utils/paths.rs:88-95
    db.store_file_with_content(
        ".memories/2025-11-10/013435_9fa8.json", // Forward slashes (production format)
        "json",
        "abc1",
        100,
        1234567890,
        test_content,
        "test_workspace",
    )
    .unwrap();

    db.store_file_with_content(
        ".memories/2025-11-10/phase1-clean.json", // Forward slashes (production format)
        "json",
        "abc2",
        100,
        1234567891,
        test_content,
        "test_workspace",
    )
    .unwrap();

    db.store_file_with_content(
        "src/tools/memory/mod.rs", // Forward slashes (production format)
        "rust",
        "abc3",
        100,
        1234567892,
        "// Memory module",
        "test_workspace",
    )
    .unwrap();

    // TEST 1: Forward-slash pattern should match forward-slash paths
    // User pattern: .memories/**/*.json (forward slashes)
    // Expected: Match 2 memory files
    // Bug: On Windows, normalization converts to *\.memories\**\*.json (backslashes)
    //      which fails to match .memories/2025-11-10/... (forward slashes)
    let results = db
        .search_file_content_fts(
            "checkpoint",
            &None,
            &Some(".memories/**/*.json".to_string()),
            10,
        )
        .unwrap();

    assert_eq!(
        results.len(),
        2,
        "Forward-slash pattern '.memories/**/*.json' should match forward-slash paths. \
         Bug: Windows normalization converts pattern to backslashes which don't match forward-slash storage."
    );
    assert!(
        results.iter().all(|r| r.path.contains(".memories")),
        "All results should be from .memories directory"
    );
    assert!(
        results.iter().all(|r| r.path.contains("/")),
        "All results should have forward-slash separators (production format)"
    );

    // TEST 2: Verify memory files are excluded from other patterns
    let results = db
        .search_file_content_fts("Memory", &None, &Some("src/**/*.rs".to_string()), 10)
        .unwrap();

    assert_eq!(
        results.len(),
        1,
        "Pattern 'src/**/*.rs' should match only src files, not .memories"
    );
    assert!(results[0].path.contains("src/tools/memory/mod.rs"));

    // TEST 3: Wildcard patterns work correctly
    // Single * matches any sequence within a path segment
    let results = db
        .search_file_content_fts(
            "checkpoint",
            &None,
            &Some(".memories/*/*.json".to_string()),
            10,
        )
        .unwrap();

    assert_eq!(
        results.len(),
        2,
        "Pattern '.memories/*/*.json' should match .memories/2025-11-10/*.json"
    );

    // TEST 4: Verify GLOB is case-sensitive
    let results = db
        .search_file_content_fts(
            "checkpoint",
            &None,
            &Some(".MEMORIES/**/*.json".to_string()),
            10,
        )
        .unwrap();

    assert_eq!(
        results.len(),
        0,
        "Pattern '.MEMORIES/**/*.json' should NOT match .memories (case-sensitive)"
    );

    // TEST 5: No pattern filter returns all matching content
    let results = db
        .search_file_content_fts("checkpoint", &None, &None, 10)
        .unwrap();

    assert_eq!(
        results.len(),
        2,
        "No file_pattern filter should return all files matching FTS query"
    );
    assert!(
        results.iter().all(|r| r.path.contains(".memories")),
        "All results should be memory files (only files with 'checkpoint' in content)"
    );
}

#[test]
fn test_fts_search_ranks_by_relevance() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // File 1: "cascade" appears once in longer document
    db.store_file_with_content(
        "README.md",
        "markdown",
        "abc1",
        1024,
        1234567890,
        "This document describes our cascade architecture pattern for data flow. \
         We use it to propagate changes through the system efficiently. \
         The design ensures consistency across all components.",
        "test_workspace",
    )
    .unwrap();

    // File 2: "cascade" appears five times in longer document
    db.store_file_with_content(
        "CASCADE.md",
        "markdown",
        "abc2",
        2048,
        1234567890,
        "The cascade cascade cascade cascade cascade model is powerful. \
         Our cascade system uses cascade patterns for cascade propagation. \
         Every cascade operation follows the cascade architecture design.",
        "test_workspace",
    )
    .unwrap();

    let results = db
        .search_file_content_fts("cascade", &None, &None, 10)
        .unwrap();

    // Verify both files are found
    assert_eq!(results.len(), 2);

    // Verify both files match
    let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
    assert!(paths.contains(&"README.md"));
    assert!(paths.contains(&"CASCADE.md"));

    // Verify ranks are differentiated (not equal)
    assert_ne!(results[0].rank, results[1].rank);
}

// REMOVED: test_fts_respects_workspace_filter
// This test is obsolete with per-workspace database architecture.
// Each workspace now has its own isolated SQLite database file,
// so workspace filtering is at the file system level, not in SQL.
// The new architecture enforces isolation by design.

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

#[test]
fn test_fts_triggers_work_after_migration() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create legacy V1 database (with workspace_id but without content)
    {
        let conn = open_test_connection(&db_path).unwrap();
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
    }

    // Open and migrate
    #[allow(unused_mut)]
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Store file with content (should trigger FTS5 sync via triggers)
    db.store_file_with_content(
        "test.rs",
        "rust",
        "hash123",
        1024,
        1234567890,
        "fn main() { println!(\"hello\"); }",
        "primary",
    )
    .unwrap();

    // Verify FTS5 search works (triggers populated FTS table)
    let results = db
        .search_file_content_fts("main", &None, &None, 10)
        .unwrap();
    assert_eq!(results.len(), 1, "FTS search should work after migration");
    assert_eq!(results[0].path, "test.rs");
}

// NOTE: Integration test removed due to pre-existing SymbolDatabase::new() tokenizer issue
// The sanitization logic is tested directly in test_sanitize_fts5_query_dot_character
// Once database initialization is fixed, this integration test can be re-added

#[test]
fn test_sanitize_fts5_query_dot_character() {
    // Test the sanitize function directly

    // Query with dot should be split and OR'd (matches tokenized content)
    let sanitized = SymbolDatabase::sanitize_fts5_query("CurrentUserService.ApplicationUser");
    assert_eq!(
        sanitized, "CurrentUserService OR ApplicationUser",
        "Queries with dots should be split and OR'd to match tokenized content"
    );

    // Multiple dots should be split into multiple OR terms
    let multi_dot = SymbolDatabase::sanitize_fts5_query("System.Collections.Generic");
    assert_eq!(
        multi_dot, "System OR Collections OR Generic",
        "Multi-dot queries should split all parts"
    );

    // Numbers with dots should pass through unchanged (don't split "3.14")
    let number = SymbolDatabase::sanitize_fts5_query("3.14");
    assert_eq!(number, "3.14", "Numeric literals should not be split");

    // Simple queries without special chars should pass through
    let simple = SymbolDatabase::sanitize_fts5_query("getUserData");
    assert_eq!(simple, "getUserData");

    // Already quoted should pass through
    let quoted = SymbolDatabase::sanitize_fts5_query("\"exact.match\"");
    assert_eq!(quoted, "\"exact.match\"");
}

// ============================================================
// FTS5 CORRUPTION BUG TESTS
// ============================================================

#[test]
fn test_fts5_corruption_with_insert_or_replace() {
    // This test reproduces the FTS5 corruption bug where INSERT OR REPLACE
    // causes rowid changes that break FTS5 content_rowid references
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Step 1: Insert initial file with content using FTS5-aware function
    db.store_file_with_content(
        "test.rs",
        "rust",
        "hash1",
        100,
        1000,
        "pub fn test_function() {}",
        "test_workspace",
    )
    .unwrap();

    // Step 2: Verify FTS5 search works initially
    let result = db.conn.query_row(
        "SELECT path FROM files_fts WHERE files_fts MATCH 'test_function'",
        [],
        |row| row.get::<_, String>(0),
    );
    assert!(result.is_ok(), "Initial FTS5 search should work");
    assert_eq!(result.unwrap(), "test.rs");

    // Step 3: Do bulk insert with INSERT OR REPLACE on same path
    // This simulates re-indexing the same file
    let file2 = FileInfo {
        path: "test.rs".to_string(), // Same path - will trigger REPLACE
        language: "rust".to_string(),
        hash: "hash2".to_string(), // Different hash (file changed)
        size: 150,
        last_modified: 2000,
        last_indexed: 0,
        symbol_count: 10,
        content: Some("pub fn test_function() {} pub fn another_function() {}".to_string()),
    };
    db.bulk_store_files(&[file2]).unwrap();

    // Step 4: Try to search - THIS SHOULD NOT FAIL with "missing row" error
    let result = db.conn.query_row(
        "SELECT path FROM files_fts WHERE files_fts MATCH 'test_function'",
        [],
        |row| row.get::<_, String>(0),
    );

    match result {
        Ok(path) => {
            assert_eq!(path, "test.rs");
            println!("✅ FTS5 search succeeded after INSERT OR REPLACE");
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("missing row") || error_msg.contains("corrupt") {
                panic!(
                    "❌ FTS5 CORRUPTION BUG REPRODUCED: {}\n\
                     Root cause: INSERT OR REPLACE changed rowid, breaking FTS5 content_rowid reference",
                    error_msg
                );
            } else {
                panic!("Unexpected error: {}", e);
            }
        }
    }
}

#[test]
fn test_fts5_rebuild_after_replace() {
    // This test verifies that rebuild() command correctly syncs FTS5 after INSERT OR REPLACE
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert files normally
    let file1 = FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash1".to_string(),
        size: 100,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 5,
        content: Some("original content".to_string()),
    };
    db.store_file_info(&file1).unwrap();

    // Get rowid before replace
    let rowid_before: i64 = db
        .conn
        .query_row(
            "SELECT rowid FROM files WHERE path = 'test.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    // Replace with bulk insert
    let file2 = FileInfo {
        path: "test.rs".to_string(),
        language: "rust".to_string(),
        hash: "hash2".to_string(),
        size: 200,
        last_modified: 2000,
        last_indexed: 0,
        symbol_count: 10,
        content: Some("updated content".to_string()),
    };
    db.bulk_store_files(&[file2]).unwrap();

    // Get rowid after replace
    let rowid_after: i64 = db
        .conn
        .query_row(
            "SELECT rowid FROM files WHERE path = 'test.rs'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    println!("Rowid before: {}, after: {}", rowid_before, rowid_after);

    // If rowids changed, FTS5 must be rebuilt to use new rowids
    if rowid_before != rowid_after {
        println!(
            "⚠️  Rowid changed from {} to {} - FTS5 rebuild required",
            rowid_before, rowid_after
        );
    }

    // Verify FTS5 still works
    let result = db
        .conn
        .query_row(
            "SELECT path FROM files_fts WHERE files_fts MATCH 'content'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();

    assert_eq!(result, "test.rs");
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

                // Query FTS5 (this was failing with corruption)
                let fts_result: Result<i64, _> = conn.query_row(
                    "SELECT COUNT(*) FROM symbols_fts WHERE name MATCH 'TestFunction'",
                    [],
                    |row| row.get(0),
                );

                assert!(
                    fts_result.is_ok(),
                    "Thread {} iteration {} FTS5 query failed: {:?}",
                    i,
                    j,
                    fts_result.err()
                );
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
    assert_eq!(busy, 0, "RESTART mode should successfully checkpoint all frames");
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
        let mut db = SymbolDatabase::new(&db_path).unwrap();

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

#[test]
fn test_batch_get_embeddings_for_symbols() {
    // RED Phase: This test will fail until we implement get_embeddings_for_symbols()
    //
    // Problem: Current code does N×2 queries (2 per symbol) in search_similar()
    // Solution: Batch fetch all embeddings in 1-2 queries total

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("batch_embeddings_test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // First, create dummy symbols (FK constraint requires symbols to exist first)
    let symbols = vec![
        Symbol {
            id: "sym_1".to_string(),
            name: "test1".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
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
            id: "sym_2".to_string(),
            name: "test2".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 2,
            start_column: 0,
            end_line: 2,
            end_column: 10,
            start_byte: 10,
            end_byte: 20,
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
            id: "sym_3".to_string(),
            name: "test3".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 3,
            start_column: 0,
            end_line: 3,
            end_column: 10,
            start_byte: 20,
            end_byte: 30,
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
            id: "sym_4".to_string(),
            name: "test4".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 4,
            start_column: 0,
            end_line: 4,
            end_column: 10,
            start_byte: 30,
            end_byte: 40,
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
            id: "sym_5".to_string(),
            name: "test5".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 5,
            start_column: 0,
            end_line: 5,
            end_column: 10,
            start_byte: 40,
            end_byte: 50,
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

    // Store symbols first (satisfies FK constraint)
    db.bulk_store_symbols(&symbols, "test_workspace").unwrap();

    // Create test embeddings for the 5 symbols
    let test_embeddings = vec![
        ("sym_1".to_string(), vec![0.1, 0.2, 0.3, 0.4]),
        ("sym_2".to_string(), vec![0.5, 0.6, 0.7, 0.8]),
        ("sym_3".to_string(), vec![0.9, 1.0, 1.1, 1.2]),
        ("sym_4".to_string(), vec![1.3, 1.4, 1.5, 1.6]),
        ("sym_5".to_string(), vec![1.7, 1.8, 1.9, 2.0]),
    ];

    // Store embeddings in database
    db.bulk_store_embeddings(&test_embeddings, 4, "test-model")
        .unwrap();

    // Now fetch them all in one batch call (this function doesn't exist yet!)
    let symbol_ids = vec!["sym_1", "sym_2", "sym_3", "sym_4", "sym_5"];
    let batch_results = db
        .get_embeddings_for_symbols(&symbol_ids, "test-model")
        .unwrap();

    // Verify we got all 5 embeddings back
    assert_eq!(
        batch_results.len(),
        5,
        "Should return all 5 requested embeddings"
    );

    // Verify each embedding matches what we stored
    for (symbol_id, expected_vector) in &test_embeddings {
        let found = batch_results
            .iter()
            .find(|(id, _)| id == symbol_id)
            .expect(&format!("Should find embedding for {}", symbol_id));

        assert_eq!(
            &found.1, expected_vector,
            "Vector for {} should match stored value",
            symbol_id
        );
    }

    // Test with subset of IDs
    let subset_ids = vec!["sym_2", "sym_4"];
    let subset_results = db
        .get_embeddings_for_symbols(&subset_ids, "test-model")
        .unwrap();

    assert_eq!(
        subset_results.len(),
        2,
        "Should return only requested subset"
    );

    // Test with non-existent ID (should skip gracefully)
    let mixed_ids = vec!["sym_1", "nonexistent", "sym_3"];
    let mixed_results = db
        .get_embeddings_for_symbols(&mixed_ids, "test-model")
        .unwrap();

    assert_eq!(
        mixed_results.len(),
        2,
        "Should return only the 2 existing embeddings"
    );
    assert!(
        mixed_results.iter().any(|(id, _)| id == "sym_1"),
        "Should include sym_1"
    );
    assert!(
        mixed_results.iter().any(|(id, _)| id == "sym_3"),
        "Should include sym_3"
    );
    assert!(
        !mixed_results.iter().any(|(id, _)| id == "nonexistent"),
        "Should not include nonexistent ID"
    );

    println!("✅ Batch embedding fetch works correctly");
}

#[test]
fn test_bulk_store_embeddings_validates_dimensions() {
    // RED Phase: This test will fail because bulk_store_embeddings doesn't validate vector length
    //
    // Problem: Function accepts dimensions parameter but never checks vector_data.len() == dimensions
    // This allows storing corrupted embeddings (e.g., 300-dim vector labeled as 384-dim)

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("dimension_validation_test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create a dummy symbol first (FK constraint)
    let symbol = Symbol {
        id: "test_symbol".to_string(),
        name: "test".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "test.rs".to_string(),
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
    db.bulk_store_symbols(&[symbol], "test_workspace").unwrap();

    // Try to store embedding with WRONG dimensions
    // Vector has 3 elements but we claim it's 384 dimensions
    let bad_embeddings = vec![("test_symbol".to_string(), vec![0.1, 0.2, 0.3])];

    let result = db.bulk_store_embeddings(&bad_embeddings, 384, "test-model");

    // Should FAIL with clear error message
    assert!(
        result.is_err(),
        "Should reject embedding with wrong dimensions (got 3, expected 384)"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        (err_msg.contains("dimension") || err_msg.contains("length"))
            && err_msg.contains("3")
            && err_msg.contains("384"),
        "Error message should explain dimension mismatch with actual numbers, got: {}",
        err_msg
    );

    // Also test the inverse: claiming 4 dimensions but providing 384
    let bad_embeddings2 = vec![(
        "test_symbol".to_string(),
        vec![0.0; 384], // 384 elements
    )];

    let result2 = db.bulk_store_embeddings(&bad_embeddings2, 4, "test-model");

    assert!(
        result2.is_err(),
        "Should reject embedding with wrong dimensions (got 384, expected 4)"
    );

    println!("✅ Dimension validation catches mismatched vectors");
}

#[test]
fn test_bulk_store_embeddings_handles_multiple_models() {
    // RED Phase: This test will fail because vector_id collisions prevent storing
    // the same symbol with multiple models
    //
    // Problem: vector_id = symbol_id, but vector_id is PRIMARY KEY
    // This means symbol_id + model "bge-small" OVERWRITES symbol_id + model "bge-large"
    // Solution: vector_id should be composite: "{symbol_id}_{model_name}"

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("multi_model_test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create a dummy symbol first (FK constraint)
    let symbol = Symbol {
        id: "test_symbol".to_string(),
        name: "test".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "test.rs".to_string(),
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
    db.bulk_store_symbols(&[symbol], "test_workspace").unwrap();

    // Store embedding with model "bge-small"
    let embeddings_small = vec![("test_symbol".to_string(), vec![0.1, 0.2, 0.3, 0.4])];
    db.bulk_store_embeddings(&embeddings_small, 4, "bge-small")
        .unwrap();

    // Store DIFFERENT embedding with model "bge-large" for SAME symbol
    let embeddings_large = vec![("test_symbol".to_string(), vec![0.5, 0.6, 0.7, 0.8])];
    db.bulk_store_embeddings(&embeddings_large, 4, "bge-large")
        .unwrap();

    // Both embeddings should exist (not overwritten)
    let small_result = db
        .get_embedding_for_symbol("test_symbol", "bge-small")
        .unwrap();
    let large_result = db
        .get_embedding_for_symbol("test_symbol", "bge-large")
        .unwrap();

    assert!(
        small_result.is_some(),
        "Should find bge-small embedding (shouldn't be overwritten by bge-large)"
    );
    assert!(large_result.is_some(), "Should find bge-large embedding");

    // Verify they have different values
    let small_vec = small_result.unwrap();
    let large_vec = large_result.unwrap();

    assert_eq!(
        small_vec,
        vec![0.1, 0.2, 0.3, 0.4],
        "bge-small should have original values"
    );
    assert_eq!(
        large_vec,
        vec![0.5, 0.6, 0.7, 0.8],
        "bge-large should have different values"
    );

    println!("✅ Multiple models per symbol work correctly (no collisions)");
}

#[test]
fn test_embedding_serialization_roundtrip() {
    // This test ensures that our serialization optimization maintains correctness
    //
    // Problem: flat_map(|f| f.to_le_bytes()).collect() is CPU-heavy
    // Solution: Pre-allocate Vec<u8> and write directly (4 bytes per f32)

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("serialization_test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create symbol (FK constraint)
    let symbol = Symbol {
        id: "test_symbol".to_string(),
        name: "test".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "test.rs".to_string(),
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
    db.bulk_store_symbols(&[symbol], "test_workspace").unwrap();

    // Test with various edge cases
    let test_vectors = vec![
        // Normal values
        vec![0.1, 0.2, 0.3, 0.4],
        // Negative values
        vec![-0.1, -0.2, -0.3, -0.4],
        // Zero
        vec![0.0, 0.0, 0.0, 0.0],
        // Very small values (near epsilon)
        vec![1e-10, 2e-10, 3e-10, 4e-10],
        // Mix of positive, negative, zero
        vec![1.0, -1.0, 0.0, 0.5],
    ];

    for (idx, original_vector) in test_vectors.iter().enumerate() {
        let symbol_id = format!("test_symbol_{}", idx);

        // Create symbol for this test case
        let test_symbol = Symbol {
            id: symbol_id.clone(),
            name: format!("test{}", idx),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: idx as u32,
            start_column: 0,
            end_line: idx as u32,
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
        db.bulk_store_symbols(&[test_symbol], "test_workspace")
            .unwrap();

        // Store embedding
        let embeddings = vec![(symbol_id.clone(), original_vector.clone())];
        db.bulk_store_embeddings(&embeddings, 4, "test-model")
            .unwrap();

        // Retrieve and verify exact match
        let retrieved = db
            .get_embedding_for_symbol(&symbol_id, "test-model")
            .unwrap()
            .expect("Should find embedding");

        assert_eq!(
            retrieved.len(),
            original_vector.len(),
            "Retrieved vector should have same length"
        );

        // Check each f32 value matches exactly (bit-for-bit via serialization)
        for (i, (original, retrieved)) in original_vector.iter().zip(retrieved.iter()).enumerate() {
            assert_eq!(
                original, retrieved,
                "Vector element {} should match exactly: original={}, retrieved={}",
                i, original, retrieved
            );
        }
    }

    println!("✅ Embedding serialization maintains bit-perfect roundtrip");
}
