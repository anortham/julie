use std::collections::HashMap;

use julie_core::database::{FileInfo, SymbolDatabase};
use julie_extractors::{
    IdentifierKind,
    base::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility},
};
use julie_test_support::db::identifier_builder;
use crate::deep_dive::data::{build_symbol_context, find_symbol};
use crate::deep_dive::deep_dive_query;
use tempfile::TempDir;

fn setup_db() -> (TempDir, SymbolDatabase) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Store file info (FK constraint requires this)
    for file in &[
        "src/engine.rs",
        "src/main.rs",
        "src/handler.rs",
        "src/tests/search_tests.rs",
    ] {
        db.store_file_info(&FileInfo {
            path: file.to_string(),
            language: "rust".to_string(),
            hash: format!("hash_{}", file),
            size: 500,
            last_modified: 1000000,
            last_indexed: 0,
            symbol_count: 2,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    (temp_dir, db)
}

fn make_symbol(
    id: &str,
    name: &str,
    kind: SymbolKind,
    file: &str,
    line: u32,
    parent_id: Option<&str>,
    signature: Option<&str>,
    visibility: Option<Visibility>,
    code_context: Option<&str>,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: "rust".to_string(),
        file_path: file.to_string(),
        start_line: line,
        end_line: line + 10,
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 100,
        parent_id: parent_id.map(|s| s.to_string()),
        signature: signature.map(|s| s.to_string()),
        doc_comment: None,
        visibility,
        metadata: None,
        semantic_group: None,
        confidence: Some(0.9),
        code_context: code_context.map(|s| s.to_string()),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

fn make_rel(
    id: &str,
    from: &str,
    to: &str,
    kind: RelationshipKind,
    file: &str,
    line: u32,
) -> Relationship {
    Relationship {
        id: id.to_string(),
        from_symbol_id: from.to_string(),
        to_symbol_id: to.to_string(),
        kind,
        file_path: file.to_string(),
        line_number: line,
        confidence: 0.9,
        metadata: None,
    }
}

mod build_context;
mod find_symbol;
mod identifiers_query_similarity;
