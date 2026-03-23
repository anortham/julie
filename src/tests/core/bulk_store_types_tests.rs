// Tests for bulk_store_types — the bulk write path for type intelligence data.
//
// Verifies: basic insert, empty input short-circuit, idempotent index
// rebuild, large batch throughput, and field-level round-trip fidelity.

use crate::database::SymbolDatabase;
use crate::database::types::FileInfo;
use crate::extractors::base::TypeInfo;
use crate::extractors::{Symbol, SymbolKind};
use std::collections::HashMap;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_file(path: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash_{}", path),
        size: 200,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 0,
        line_count: 0,
        content: None,
    }
}

fn make_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 10,
        end_column: 0,
        start_byte: 0,
        end_byte: 100,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    }
}

fn make_type_info(symbol_id: &str, resolved_type: &str) -> TypeInfo {
    TypeInfo {
        symbol_id: symbol_id.to_string(),
        resolved_type: resolved_type.to_string(),
        generic_params: None,
        constraints: None,
        is_inferred: false,
        language: "rust".to_string(),
        metadata: None,
    }
}

/// Set up a fresh database with prerequisite file + symbol rows so that FK
/// constraints on the `types` table are satisfied.
fn setup_db_with_prerequisites(symbol_count: usize) -> (TempDir, SymbolDatabase, Vec<String>) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    let file = make_file("src/lib.rs");
    db.bulk_store_files(&[file]).unwrap();

    let mut symbol_ids = Vec::with_capacity(symbol_count);
    let symbols: Vec<Symbol> = (0..symbol_count)
        .map(|i| {
            let id = format!("sym_{}", i);
            symbol_ids.push(id.clone());
            make_symbol(&id, &format!("func_{}", i), "src/lib.rs")
        })
        .collect();
    db.bulk_store_symbols(&symbols, "ws_test").unwrap();

    (tmp, db, symbol_ids)
}

fn count_types(db: &SymbolDatabase) -> i64 {
    db.conn
        .query_row("SELECT COUNT(*) FROM types", [], |row| row.get(0))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Test 1: Basic insert — 3 types, verify count and individual field values
// ---------------------------------------------------------------------------
#[test]
fn test_bulk_store_types_basic_insert() {
    let (_tmp, mut db, ids) = setup_db_with_prerequisites(3);

    let types = vec![
        {
            let mut t = make_type_info(&ids[0], "Result<(), Error>");
            t.generic_params = Some(vec!["T".into(), "E".into()]);
            t.constraints = Some(vec!["T: Clone".into()]);
            t.is_inferred = false;
            t.metadata = Some({
                let mut m = HashMap::new();
                m.insert("origin".into(), serde_json::json!("explicit"));
                m
            });
            t
        },
        {
            let mut t = make_type_info(&ids[1], "Vec<String>");
            t.is_inferred = true;
            t
        },
        make_type_info(&ids[2], "i32"),
    ];

    db.bulk_store_types(&types, "ws_test")
        .expect("bulk_store_types should succeed");

    // Row count
    assert_eq!(count_types(&db), 3);

    // Spot-check first type: all columns round-trip correctly
    let (resolved, generic, constraints, inferred, lang, meta): (
        String,
        Option<String>,
        Option<String>,
        i32,
        String,
        Option<String>,
    ) = db
        .conn
        .query_row(
            "SELECT resolved_type, generic_params, constraints, is_inferred, language, metadata \
             FROM types WHERE symbol_id = ?1",
            [&ids[0]],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("query first type");

    assert_eq!(resolved, "Result<(), Error>");
    assert_eq!(
        generic.as_deref(),
        Some(r#"["T","E"]"#),
        "generic_params JSON mismatch"
    );
    assert_eq!(
        constraints.as_deref(),
        Some(r#"["T: Clone"]"#),
        "constraints JSON mismatch"
    );
    assert_eq!(inferred, 0, "is_inferred should be 0 for explicit");
    assert_eq!(lang, "rust");
    assert!(
        meta.as_ref().unwrap().contains("explicit"),
        "metadata should contain 'explicit'"
    );

    // Verify the inferred flag on the second type
    let inferred_val: i32 = db
        .conn
        .query_row(
            "SELECT is_inferred FROM types WHERE symbol_id = ?1",
            [&ids[1]],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(inferred_val, 1, "is_inferred should be 1 for inferred type");
}

// ---------------------------------------------------------------------------
// Test 2: Empty input — returns Ok, no rows inserted
// ---------------------------------------------------------------------------
#[test]
fn test_bulk_store_types_empty_input() {
    let (_tmp, mut db, _ids) = setup_db_with_prerequisites(0);

    db.bulk_store_types(&[], "ws_test")
        .expect("empty input should succeed");

    assert_eq!(count_types(&db), 0);
}

// ---------------------------------------------------------------------------
// Test 3: Idempotent indexes — calling twice doesn't cause "index already
// exists" errors (indexes are dropped and recreated each call)
// ---------------------------------------------------------------------------
#[test]
fn test_bulk_store_types_idempotent_indexes() {
    let (_tmp, mut db, ids) = setup_db_with_prerequisites(2);

    let types = vec![
        make_type_info(&ids[0], "String"),
        make_type_info(&ids[1], "u64"),
    ];

    db.bulk_store_types(&types, "ws_test")
        .expect("first call should succeed");
    assert_eq!(count_types(&db), 2);

    // Second call with INSERT OR REPLACE should upsert cleanly
    let types2 = vec![
        make_type_info(&ids[0], "String"),      // same
        make_type_info(&ids[1], "Option<u64>"), // updated resolved_type
    ];
    db.bulk_store_types(&types2, "ws_test")
        .expect("second call should succeed without index errors");

    assert_eq!(count_types(&db), 2, "count should remain 2 after upsert");

    // Verify the updated value
    let resolved: String = db
        .conn
        .query_row(
            "SELECT resolved_type FROM types WHERE symbol_id = ?1",
            [&ids[1]],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(resolved, "Option<u64>", "should reflect upserted value");
}

// ---------------------------------------------------------------------------
// Test 4: Large batch — 200 types, verify count and no errors
// ---------------------------------------------------------------------------
#[test]
fn test_bulk_store_types_large_batch() {
    let count = 200;
    let (_tmp, mut db, ids) = setup_db_with_prerequisites(count);

    let types: Vec<TypeInfo> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let mut t = make_type_info(id, &format!("Type_{}", i));
            if i % 3 == 0 {
                t.generic_params = Some(vec![format!("T{}", i)]);
            }
            if i % 5 == 0 {
                t.is_inferred = true;
            }
            t
        })
        .collect();

    db.bulk_store_types(&types, "ws_test")
        .expect("large batch should succeed");

    assert_eq!(count_types(&db), count as i64);

    // Spot-check a few entries
    let resolved: String = db
        .conn
        .query_row(
            "SELECT resolved_type FROM types WHERE symbol_id = ?1",
            [&ids[0]],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(resolved, "Type_0");

    let resolved_last: String = db
        .conn
        .query_row(
            "SELECT resolved_type FROM types WHERE symbol_id = ?1",
            [&ids[count - 1]],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(resolved_last, format!("Type_{}", count - 1));
}
