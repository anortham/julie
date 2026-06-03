// TDD Phase 2: Write failing test for bulk_store_types
// This test will fail until we implement the bulk_store_types function

use crate::database::*;
use julie_extractors::base::TypeInfo;
use std::collections::HashMap;
use tempfile::TempDir;

#[test]
fn test_bulk_store_types() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("types_test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // First, create and store some symbols (types are linked to symbols)
    let symbols = vec![
        julie_extractors::Symbol {
            id: "symbol1".to_string(),
            name: "getUserData".to_string(),
            kind: julie_extractors::SymbolKind::Function,
            language: "python".to_string(),
            file_path: "test.py".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: Some("def getUserData() -> Dict[str, Any]".to_string()),
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
        julie_extractors::Symbol {
            id: "symbol2".to_string(),
            name: "processData".to_string(),
            kind: julie_extractors::SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "test.ts".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: Some("function processData<T>(data: T): Promise<T>".to_string()),
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

    // Store file info first (foreign key dependency)
    let file_info1 = FileInfo {
        path: "test.py".to_string(),
        language: "python".to_string(),
        hash: "hash1".to_string(),
        size: 100,
        last_modified: 123456,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    };
    let file_info2 = FileInfo {
        path: "test.ts".to_string(),
        language: "typescript".to_string(),
        hash: "hash2".to_string(),
        size: 100,
        last_modified: 123456,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 0,
        content: None,
    };
    db.bulk_store_files(&[file_info1, file_info2]).unwrap();
    db.bulk_store_symbols(&symbols, "test_workspace").unwrap();

    // Create TypeInfo objects
    let types = vec![
        TypeInfo {
            symbol_id: "symbol1".to_string(),
            resolved_type: "Dict[str, Any]".to_string(),
            generic_params: None,
            constraints: None,
            is_inferred: false,
            language: "python".to_string(),
            metadata: None,
        },
        TypeInfo {
            symbol_id: "symbol2".to_string(),
            resolved_type: "Promise<T>".to_string(),
            generic_params: Some(vec!["T".to_string()]),
            constraints: None,
            is_inferred: true,
            language: "typescript".to_string(),
            metadata: Some({
                let mut map = HashMap::new();
                map.insert("async".to_string(), serde_json::json!(true));
                map
            }),
        },
    ];

    // THIS WILL FAIL - bulk_store_types doesn't exist yet
    db.bulk_store_types(&types, "test_workspace").unwrap();

    // Verify types were stored
    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM types", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 2, "Expected 2 types to be stored");

    // Verify first type
    let (resolved_type, language, is_inferred): (String, String, i32) = db
        .conn
        .query_row(
            "SELECT resolved_type, language, is_inferred FROM types WHERE symbol_id = ?",
            ["symbol1"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(resolved_type, "Dict[str, Any]");
    assert_eq!(language, "python");
    assert_eq!(is_inferred, 0); // false = 0

    // Verify second type with generics
    let (resolved_type, generic_params, is_inferred): (String, Option<String>, i32) = db
        .conn
        .query_row(
            "SELECT resolved_type, generic_params, is_inferred FROM types WHERE symbol_id = ?",
            ["symbol2"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(resolved_type, "Promise<T>");
    assert_eq!(is_inferred, 1); // true = 1

    // Verify generic_params JSON
    let params: Vec<String> = serde_json::from_str(&generic_params.unwrap()).unwrap();
    assert_eq!(params, vec!["T"]);
}

#[test]
fn test_bulk_store_types_performance() {
    // Test that bulk_store_types uses index dropping for performance
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("types_perf_test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create 1000 types
    let mut types = Vec::new();
    let mut symbols = Vec::new();
    let mut file_infos = Vec::new();

    for i in 0..1000 {
        let symbol_id = format!("symbol_{}", i);
        let file_path = format!("file_{}.py", i % 10);

        // Add file info if not already added
        if i % 100 == 0 {
            file_infos.push(FileInfo {
                path: file_path.clone(),
                language: "python".to_string(),
                hash: format!("hash_{}", i),
                size: 100,
                last_modified: 123456,
                last_indexed: 0,
                symbol_count: 100,
                line_count: 0,
                content: None,
            });
        }

        symbols.push(julie_extractors::Symbol {
            id: symbol_id.clone(),
            name: format!("func_{}", i),
            kind: julie_extractors::SymbolKind::Function,
            language: "python".to_string(),
            file_path: file_path,
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: Some(format!("def func_{}() -> str", i)),
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
        });

        types.push(TypeInfo {
            symbol_id: symbol_id.clone(),
            resolved_type: "str".to_string(),
            generic_params: None,
            constraints: None,
            is_inferred: i % 2 == 0, // Alternate inferred/explicit
            language: "python".to_string(),
            metadata: None,
        });
    }

    db.bulk_store_files(&file_infos).unwrap();
    db.bulk_store_symbols(&symbols, "test_workspace").unwrap();

    // Bulk store should complete quickly (indexes dropped during insert)
    let start = std::time::Instant::now();
    db.bulk_store_types(&types, "test_workspace").unwrap();
    let duration = start.elapsed();

    // Should complete in < 1 second for 1000 types
    assert!(
        duration.as_secs() < 1,
        "Bulk insert took too long: {:?}",
        duration
    );

    // Verify all types were stored
    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM types", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1000, "Expected 1000 types to be stored");
}
