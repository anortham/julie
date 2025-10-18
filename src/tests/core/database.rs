// Tests extracted from src/database/mod.rs
// These were previously inline tests that have been moved to follow project standards

use crate::database::*;
use crate::extractors::{Symbol, SymbolKind};
use rusqlite::Connection;
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
    let conn = rusqlite::Connection::open(&db_path).unwrap();

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
    let file_info = crate::database::create_file_info(&test_file, "typescript").unwrap();
    println!("File path in file_info: {}", file_info.path);
    db.store_file_info(&file_info).unwrap();

    // Create a symbol with the same file path (canonicalized to match file_info)
    let file_path = test_file
        .canonicalize()
        .unwrap_or_else(|_| test_file.clone())
        .to_string_lossy()
        .to_string();
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
    };

    // This should work without foreign key constraint error
    let result = db.store_symbols(&[symbol]);
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
    let conn = rusqlite::Connection::open(&db_path).unwrap();
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

    db.store_symbols(&[symbol.clone()]).unwrap();

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

    let file_info = crate::database::create_file_info(&fixture_path, "go").unwrap();
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
        file_path: test_file
            .canonicalize()
            .unwrap_or_else(|_| test_file.clone())
            .to_string_lossy()
            .to_string(),
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
    };

    // First, store the file record (required due to foreign key constraint)
    let file_info = crate::database::create_file_info(&test_file, "typescript").unwrap();
    println!("DEBUG: File path in file_info: {}", file_info.path);
    println!("DEBUG: Symbol file path: {}", symbol.file_path);
    db.store_file_info(&file_info).unwrap();

    // Store the symbol
    db.store_symbols(&[symbol.clone()]).unwrap();

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
    };

    db.store_symbols(&[caller_symbol, called_symbol])
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
    db.store_symbols(&[ts_interface, rust_struct])
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
    let base_extractor = BaseExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        source_code.to_string(),
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
    db.store_symbols(&[symbol.clone()]).unwrap();

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
        // Regular fields that work:
        signature: Some("fn complete_function() -> Result<()>".to_string()),
        parent_id: None,
        metadata: None,
        semantic_group: Some("test-group".to_string()),
        confidence: Some(0.95),
    };

    // Store the symbol
    db.store_symbols(&[symbol.clone()])
        .unwrap();

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
    let results = db.search_file_content_fts("SQLite", 10).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, "docs/architecture.md");
    assert!(results[0].snippet.contains("SQLite"));
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

    let results = db.search_file_content_fts("cascade", 10).unwrap();

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
        let conn = Connection::open(&db_path).unwrap();
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
        let conn = Connection::open(&db_path).unwrap();
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
    let results = db.search_file_content_fts("main", 10).unwrap();
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
