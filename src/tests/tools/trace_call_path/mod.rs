// Tests extracted from src/tools/trace_call_path.rs
// These were previously inline tests that have been moved to follow project standards

mod comprehensive; // Parameter validation and API tests
mod new_features; // TDD tests for new features (output_format, configurable parameters)
mod workspace_isolation; // TDD tests for workspace isolation bug fix

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind};
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
        content_type: None,
    }
}

/// Regression test: direction=both must trace BOTH upstream and downstream.
/// Previously, a shared visited set caused the second direction to return empty
/// because the starting symbol was already marked as visited.
#[tokio::test]
async fn direction_both_returns_upstream_and_downstream() {
    use crate::tools::trace_call_path::tracing;

    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("test.db");
    let db = SymbolDatabase::new(db_path).expect("db");
    let db = Arc::new(Mutex::new(db));

    // Setup: A → B → C (A calls B, B calls C)
    let sym_a = make_symbol("sym_a", "caller_func", "rust", "src/caller.rs");
    let sym_b = make_symbol("sym_b", "middle_func", "rust", "src/middle.rs");
    let sym_c = make_symbol("sym_c", "callee_func", "rust", "src/callee.rs");

    {
        let mut db_guard = db.lock().unwrap();

        // Store files
        for (sym, hash) in [(&sym_a, "h1"), (&sym_b, "h2"), (&sym_c, "h3")] {
            db_guard
                .store_file_info(&FileInfo {
                    path: sym.file_path.clone(),
                    language: sym.language.clone(),
                    hash: hash.to_string(),
                    size: 0,
                    last_modified: 0,
                    last_indexed: 0,
                    symbol_count: 1,
                    content: Some("".to_string()),
                })
                .expect("store file");
        }

        db_guard
            .store_symbols_transactional(&[sym_a.clone(), sym_b.clone(), sym_c.clone()])
            .expect("store symbols");

        // A calls B (upstream of B)
        let rel_a_to_b = Relationship {
            id: "rel_a_b".to_string(),
            from_symbol_id: sym_a.id.clone(),
            to_symbol_id: sym_b.id.clone(),
            kind: RelationshipKind::Calls,
            file_path: sym_a.file_path.clone(),
            line_number: 10,
            confidence: 1.0,
            metadata: None,
        };

        // B calls C (downstream of B)
        let rel_b_to_c = Relationship {
            id: "rel_b_c".to_string(),
            from_symbol_id: sym_b.id.clone(),
            to_symbol_id: sym_c.id.clone(),
            kind: RelationshipKind::Calls,
            file_path: sym_b.file_path.clone(),
            line_number: 20,
            confidence: 1.0,
            metadata: None,
        };

        db_guard
            .store_relationships(&[rel_a_to_b, rel_b_to_c])
            .expect("store relationships");
    }

    // Trace B with direction=both
    let mut visited = std::collections::HashSet::new();

    // Clone visited sets like call_tool does for direction=both
    let mut upstream_visited = visited.clone();
    let mut downstream_visited = visited.clone();

    let upstream = tracing::trace_upstream(
        &db,
        &sym_b,
        0,
        &mut upstream_visited,
        3,
    )
    .await
    .expect("trace upstream");

    let downstream = tracing::trace_downstream(
        &db,
        &sym_b,
        0,
        &mut downstream_visited,
        3,
    )
    .await
    .expect("trace downstream");

    visited.extend(upstream_visited);
    visited.extend(downstream_visited);

    // Both directions should return results
    assert!(
        !upstream.is_empty(),
        "upstream should find caller_func (A calls B)"
    );
    assert!(
        !downstream.is_empty(),
        "downstream should find callee_func (B calls C)"
    );

    // Verify specific symbols
    assert_eq!(upstream[0].symbol.name, "caller_func");
    assert_eq!(downstream[0].symbol.name, "callee_func");
}

/// Test that identifier-based caller discovery finds callers that
/// the relationships table misses. Setup: A calls B but there's NO
/// relationship record — only an identifier with kind=Call and
/// containing_symbol_id=A.
#[tokio::test]
async fn identifier_based_caller_discovery_supplements_relationships() {
    use crate::tools::trace_call_path::tracing;

    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("test.db");
    let db = SymbolDatabase::new(db_path).expect("db");
    let db = Arc::new(Mutex::new(db));

    // A is the caller, B is the target — but NO relationship between them
    let sym_a = make_symbol("sym_a", "controller_handler", "rust", "src/controller.rs");
    let sym_b = make_symbol("sym_b", "process_order", "rust", "src/service.rs");

    {
        let mut db_guard = db.lock().unwrap();

        for (sym, hash) in [(&sym_a, "h1"), (&sym_b, "h2")] {
            db_guard
                .store_file_info(&FileInfo {
                    path: sym.file_path.clone(),
                    language: sym.language.clone(),
                    hash: hash.to_string(),
                    size: 0,
                    last_modified: 0,
                    last_indexed: 0,
                    symbol_count: 1,
                    content: Some("".to_string()),
                })
                .expect("store file");
        }

        db_guard
            .store_symbols_transactional(&[sym_a.clone(), sym_b.clone()])
            .expect("store symbols");

        // NO relationships stored — the identifier table is the only link.
        // Store an identifier: A contains a call to "process_order"
        let ident = Identifier {
            id: "ident_1".to_string(),
            name: "process_order".to_string(),
            kind: IdentifierKind::Call,
            language: "rust".to_string(),
            file_path: sym_a.file_path.clone(),
            start_line: 15,
            start_column: 4,
            end_line: 15,
            end_column: 20,
            start_byte: 100,
            end_byte: 116,
            containing_symbol_id: Some(sym_a.id.clone()),
            target_symbol_id: None,
            confidence: 0.9,
            code_context: None,
        };

        db_guard
            .bulk_store_identifiers(&[ident], "primary")
            .expect("store identifiers");
    }

    // Trace upstream from B — should find A via identifiers even without relationships
    let mut visited = std::collections::HashSet::new();
    let upstream = tracing::trace_upstream(&db, &sym_b, 0, &mut visited, 3)
        .await
        .expect("trace upstream");

    assert!(
        !upstream.is_empty(),
        "identifier-based discovery should find controller_handler as a caller of process_order"
    );
    assert_eq!(upstream[0].symbol.name, "controller_handler");
    assert_eq!(upstream[0].symbol.id, "sym_a");
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
            .store_file_info(&file_target)
            .expect("store target file");
        db_guard
            .store_file_info(&file_variant)
            .expect("store variant file");

        db_guard
            .store_symbols_transactional(&[target.clone(), variant.clone(), other.clone()])
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
        context_file: None,
        workspace: Some(workspace_id.to_string()),
        output_format: Some("json".to_string()),
    };

    let callers = tool
        .find_cross_language_symbols(&db, &target)
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
