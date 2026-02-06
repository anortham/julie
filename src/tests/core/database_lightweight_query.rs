// Tests for lightweight symbol queries (structure mode optimization)
//
// The lightweight query skips expensive columns (code_context, metadata,
// semantic_group, confidence, content_type) that are immediately discarded
// when mode="structure". This avoids wasted serde_json::from_str() calls
// and large code_context column reads.

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::base::{Symbol, SymbolKind};
use tempfile::TempDir;

/// Helper to create a test database with a few symbols including metadata and code_context
fn setup_test_db_with_rich_symbols() -> (TempDir, SymbolDatabase) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Store file info first (foreign key constraint)
    let file_info = FileInfo {
        path: "src/main.rs".to_string(),
        language: "rust".to_string(),
        hash: "abc123".to_string(),
        size: 500,
        last_modified: 1000000,
        last_indexed: 0,
        symbol_count: 3,
        content: None,
    };
    db.store_file_info(&file_info).unwrap();

    // Store symbols with rich data in expensive columns
    let symbols = vec![
        Symbol {
            id: "sym-parent-1".to_string(),
            name: "UserService".to_string(),
            kind: SymbolKind::Class,
            language: "rust".to_string(),
            file_path: "src/main.rs".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 50,
            end_column: 1,
            start_byte: 0,
            end_byte: 500,
            signature: Some("pub struct UserService".to_string()),
            doc_comment: Some("User service for managing users".to_string()),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: None,
            metadata: Some({
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "is_async".to_string(),
                    serde_json::Value::Bool(true),
                );
                m
            }),
            semantic_group: Some("service".to_string()),
            confidence: Some(0.95),
            code_context: Some("pub struct UserService {\n    users: Vec<User>,\n}".to_string()),
            content_type: None,
        },
        Symbol {
            id: "sym-child-1".to_string(),
            name: "get_user".to_string(),
            kind: SymbolKind::Method,
            language: "rust".to_string(),
            file_path: "src/main.rs".to_string(),
            start_line: 10,
            start_column: 4,
            end_line: 20,
            end_column: 5,
            start_byte: 100,
            end_byte: 300,
            signature: Some("pub fn get_user(&self, id: u64) -> Option<&User>".to_string()),
            doc_comment: None,
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: Some("sym-parent-1".to_string()),
            metadata: Some({
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "return_type".to_string(),
                    serde_json::Value::String("Option<&User>".to_string()),
                );
                m
            }),
            semantic_group: Some("accessor".to_string()),
            confidence: Some(0.9),
            code_context: Some(
                "pub fn get_user(&self, id: u64) -> Option<&User> {\n    self.users.get(&id)\n}"
                    .to_string(),
            ),
            content_type: None,
        },
        Symbol {
            id: "sym-child-2".to_string(),
            name: "add_user".to_string(),
            kind: SymbolKind::Method,
            language: "rust".to_string(),
            file_path: "src/main.rs".to_string(),
            start_line: 22,
            start_column: 4,
            end_line: 30,
            end_column: 5,
            start_byte: 310,
            end_byte: 490,
            signature: Some("pub fn add_user(&mut self, user: User)".to_string()),
            doc_comment: Some("Add a user to the service".to_string()),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: Some("sym-parent-1".to_string()),
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: Some("pub fn add_user(&mut self, user: User) {\n    self.users.push(user);\n}".to_string()),
            content_type: None,
        },
    ];

    db.store_symbols_transactional(&symbols).unwrap();

    (temp_dir, db)
}

#[test]
fn test_lightweight_query_returns_same_core_fields() {
    let (_temp_dir, db) = setup_test_db_with_rich_symbols();

    // Both queries should return the same symbols
    let full = db.get_symbols_for_file("src/main.rs").unwrap();
    let lightweight = db.get_symbols_for_file_lightweight("src/main.rs").unwrap();

    assert_eq!(full.len(), lightweight.len(), "Both queries should return same number of symbols");

    // Core fields must match exactly
    for (f, l) in full.iter().zip(lightweight.iter()) {
        assert_eq!(f.id, l.id, "id mismatch");
        assert_eq!(f.name, l.name, "name mismatch");
        assert_eq!(f.kind, l.kind, "kind mismatch");
        assert_eq!(f.language, l.language, "language mismatch");
        assert_eq!(f.file_path, l.file_path, "file_path mismatch");
        assert_eq!(f.signature, l.signature, "signature mismatch");
        assert_eq!(f.start_line, l.start_line, "start_line mismatch");
        assert_eq!(f.start_column, l.start_column, "start_column mismatch");
        assert_eq!(f.end_line, l.end_line, "end_line mismatch");
        assert_eq!(f.end_column, l.end_column, "end_column mismatch");
        assert_eq!(f.start_byte, l.start_byte, "start_byte mismatch");
        assert_eq!(f.end_byte, l.end_byte, "end_byte mismatch");
        assert_eq!(f.doc_comment, l.doc_comment, "doc_comment mismatch");
        assert_eq!(f.visibility, l.visibility, "visibility mismatch");
        assert_eq!(f.parent_id, l.parent_id, "parent_id mismatch");
    }
}

#[test]
fn test_lightweight_query_skips_expensive_fields() {
    let (_temp_dir, db) = setup_test_db_with_rich_symbols();

    let full = db.get_symbols_for_file("src/main.rs").unwrap();
    let lightweight = db.get_symbols_for_file_lightweight("src/main.rs").unwrap();

    // Full query should have the rich data
    let parent_full = full.iter().find(|s| s.id == "sym-parent-1").unwrap();
    assert!(parent_full.metadata.is_some(), "Full query should have metadata");
    assert!(parent_full.code_context.is_some(), "Full query should have code_context");
    assert!(parent_full.semantic_group.is_some(), "Full query should have semantic_group");
    assert!(parent_full.confidence.is_some(), "Full query should have confidence");

    // Lightweight query should have None for all expensive fields
    for sym in &lightweight {
        assert!(sym.metadata.is_none(), "Lightweight should skip metadata for {}", sym.name);
        assert!(
            sym.code_context.is_none(),
            "Lightweight should skip code_context for {}",
            sym.name
        );
        assert!(
            sym.semantic_group.is_none(),
            "Lightweight should skip semantic_group for {}",
            sym.name
        );
        assert!(sym.confidence.is_none(), "Lightweight should skip confidence for {}", sym.name);
        assert!(
            sym.content_type.is_none(),
            "Lightweight should skip content_type for {}",
            sym.name
        );
    }
}

#[test]
fn test_lightweight_query_preserves_ordering() {
    let (_temp_dir, db) = setup_test_db_with_rich_symbols();

    let lightweight = db.get_symbols_for_file_lightweight("src/main.rs").unwrap();

    // Should be ordered by start_line, start_col (same as full query)
    assert_eq!(lightweight[0].name, "UserService");
    assert_eq!(lightweight[1].name, "get_user");
    assert_eq!(lightweight[2].name, "add_user");
}

#[test]
fn test_lightweight_query_empty_file() {
    let (_temp_dir, db) = setup_test_db_with_rich_symbols();

    let lightweight = db
        .get_symbols_for_file_lightweight("nonexistent.rs")
        .unwrap();
    assert!(lightweight.is_empty(), "Nonexistent file should return empty");
}
