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

