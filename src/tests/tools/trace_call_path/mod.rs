// Tests extracted from src/tools/trace_call_path.rs
// These were previously inline tests that have been moved to follow project standards

mod comprehensive; // Parameter validation and API tests
mod new_features; // TDD tests for new features (output_format, configurable parameters)

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::tools::trace_call_path::TraceCallPathTool;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

fn make_symbol(id: &str, name: &str, language: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: language.to_string(),
        file_path: file_path.to_string(),
        signature: None,
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 1,
        start_byte: 0,
        end_byte: 1,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
    }
}

#[tokio::test]
async fn cross_language_callers_found_via_naming_variant() {
    let workspace_id = "primary";
    let temp = tempdir().expect("tempdir");
    let db_path = temp.path().join("test.db");
    let db = SymbolDatabase::new(db_path).expect("db");
    let db = Arc::new(Mutex::new(db));

    let target = make_symbol("target", "process_payment", "python", "app.py");
    let variant = make_symbol("variant", "ProcessPayment", "csharp", "Payment.cs");
    let other = make_symbol("other", "helper", "csharp", "Payment.cs");

    {
        let mut db_guard = db.lock().unwrap();
        let file_target = FileInfo {
            path: target.file_path.clone(),
            language: target.language.clone(),
            hash: "hash1".to_string(),
            size: 0,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 1,
            content: Some("".to_string()),
        };

        let file_variant = FileInfo {
            path: variant.file_path.clone(),
            language: variant.language.clone(),
            hash: "hash2".to_string(),
            size: 0,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 1,
            content: Some("".to_string()),
        };

        db_guard
            .store_file_info(&file_target, workspace_id)
            .expect("store target file");
        db_guard
            .store_file_info(&file_variant, workspace_id)
            .expect("store variant file");

        db_guard
            .store_symbols(
                &[target.clone(), variant.clone(), other.clone()],
                workspace_id,
            )
            .expect("store symbols");

        // Note: No relationship needed - naming variant is sufficient for cross-language matching
        let rel = Relationship {
            id: "rel1".to_string(),
            from_symbol_id: variant.id.clone(),
            to_symbol_id: other.id.clone(),
            kind: RelationshipKind::Calls,
            file_path: variant.file_path.clone(),
            line_number: 10,
            confidence: 1.0,
            metadata: None,
        };

        db_guard
            .store_relationships(&[rel])
            .expect("store relationships");
    }

    let tool = TraceCallPathTool {
        symbol: target.name.clone(),
        direction: "upstream".to_string(),
        max_depth: 3,
        cross_language: true,
        similarity_threshold: 0.7,
        context_file: None,
        workspace: Some(workspace_id.to_string()),
        output_format: "json".to_string(),
        semantic_limit: None,
        cross_language_max_depth: None,
    };

    let callers = tool
        .find_cross_language_callers(&db, &target)
        .await
        .expect("callers");

    // NEW BEHAVIOR: Naming variant match is sufficient - no database relationship required!
    assert_eq!(
        callers.len(),
        1,
        "Expected to find cross-language caller via naming variant"
    );
    assert_eq!(callers[0].name, "ProcessPayment");
    assert_eq!(callers[0].language, "csharp");
}
