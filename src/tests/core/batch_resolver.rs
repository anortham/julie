// Tests for batch pending relationship resolution
//
// Verifies that the batch resolution approach (group by callee_name,
// query once per unique name) produces identical results to the
// sequential per-relationship approach — just much faster.

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::base::{
    Identifier, IdentifierKind, PendingRelationship, RelationshipKind, Symbol, SymbolKind,
    Visibility,
};
use crate::tools::workspace::indexing::resolver;
use tempfile::TempDir;

/// Helper: minimal symbol with just the fields that matter for resolution
fn sym(id: &str, name: &str, kind: SymbolKind, lang: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: lang.to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 10,
        end_column: 1,
        start_byte: 0,
        end_byte: 100,
        signature: None,
        doc_comment: None,
        visibility: Some(Visibility::Public),
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    }
}

/// Helper: minimal pending relationship
fn pending(from_id: &str, callee: &str, file_path: &str) -> PendingRelationship {
    PendingRelationship {
        from_symbol_id: from_id.to_string(),
        callee_name: callee.to_string(),
        kind: RelationshipKind::Calls,
        file_path: file_path.to_string(),
        line_number: 10,
        confidence: 0.8,
    }
}

/// Helper: set up a test DB with symbols across multiple files
fn setup_test_db() -> (TempDir, SymbolDatabase) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Two source files
    for (path, lang) in &[
        ("src/auth.rs", "rust"),
        ("src/db.rs", "rust"),
        ("src/utils.ts", "typescript"),
    ] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: lang.to_string(),
            hash: "h".to_string(),
            size: 100,
            last_modified: 1000,
            last_indexed: 0,
            symbol_count: 2,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    let symbols = vec![
        sym(
            "s1",
            "authenticate",
            SymbolKind::Function,
            "rust",
            "src/auth.rs",
        ),
        sym(
            "s2",
            "hash_password",
            SymbolKind::Function,
            "rust",
            "src/auth.rs",
        ),
        sym("s3", "query", SymbolKind::Function, "rust", "src/db.rs"),
        sym("s4", "connect", SymbolKind::Function, "rust", "src/db.rs"),
        // Same-named symbol in a different language (disambiguation test)
        sym(
            "s5",
            "authenticate",
            SymbolKind::Function,
            "typescript",
            "src/utils.ts",
        ),
    ];
    db.store_symbols_transactional(&symbols).unwrap();

    (temp_dir, db)
}

// ─────────────────────────────────────────────────────────────────────
// Tests for find_symbols_by_names_batch
// ─────────────────────────────────────────────────────────────────────

#[test]
fn test_batch_query_returns_all_matching_symbols() {
    let (_tmp, db) = setup_test_db();

    let names = vec!["authenticate".to_string(), "query".to_string()];
    let result = db.find_symbols_by_names_batch(&names).unwrap();

    // "authenticate" has 2 symbols (rust + typescript), "query" has 1
    assert_eq!(result.len(), 2, "should have entries for both names");
    assert_eq!(result["authenticate"].len(), 2);
    assert_eq!(result["query"].len(), 1);
    assert_eq!(result["query"][0].id, "s3");
}

#[test]
fn test_batch_query_missing_names_are_absent() {
    let (_tmp, db) = setup_test_db();

    let names = vec!["authenticate".to_string(), "nonexistent".to_string()];
    let result = db.find_symbols_by_names_batch(&names).unwrap();

    assert!(result.contains_key("authenticate"));
    assert!(!result.contains_key("nonexistent"));
}

#[test]
fn test_batch_query_empty_input() {
    let (_tmp, db) = setup_test_db();

    let names: Vec<String> = vec![];
    let result = db.find_symbols_by_names_batch(&names).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_batch_query_deduplicates_input_names() {
    let (_tmp, db) = setup_test_db();

    // Same name twice — should only query once, return same results
    let names = vec!["authenticate".to_string(), "authenticate".to_string()];
    let result = db.find_symbols_by_names_batch(&names).unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result["authenticate"].len(), 2);
}

// ─────────────────────────────────────────────────────────────────────
// Tests for resolve_batch
// ─────────────────────────────────────────────────────────────────────

#[test]
fn test_resolve_batch_resolves_multiple_pendings_with_shared_callee() {
    let (_tmp, db) = setup_test_db();

    // Two callers both call "authenticate" — should resolve with a single DB lookup
    let pendings = vec![
        pending("caller-1", "authenticate", "src/db.rs"),
        pending("caller-2", "authenticate", "src/auth.rs"),
    ];

    let (resolved, stats) = resolver::resolve_batch(&pendings, &db);

    assert_eq!(stats.total, 2);
    assert_eq!(stats.resolved, 2);
    assert_eq!(resolved.len(), 2);
    // Both should resolve to the rust "authenticate" (s1) due to same-language preference
    for rel in &resolved {
        assert_eq!(rel.to_symbol_id, "s1");
    }
}

#[test]
fn test_resolve_batch_handles_no_candidates() {
    let (_tmp, db) = setup_test_db();

    let pendings = vec![pending("caller-1", "nonexistent_func", "src/db.rs")];

    let (resolved, stats) = resolver::resolve_batch(&pendings, &db);

    assert!(resolved.is_empty());
    assert_eq!(stats.no_candidates, 1);
}

#[test]
fn test_resolve_batch_matches_sequential_resolution() {
    let (_tmp, db) = setup_test_db();

    let pendings = vec![
        pending("c1", "authenticate", "src/auth.rs"),
        pending("c2", "query", "src/db.rs"),
        pending("c3", "connect", "src/db.rs"),
        pending("c4", "ghost_func", "src/auth.rs"), // no match
        pending("c5", "authenticate", "src/db.rs"),
    ];

    // Batch resolution
    let (batch_resolved, batch_stats) = resolver::resolve_batch(&pendings, &db);

    // Sequential resolution (the old way)
    let mut seq_resolved = Vec::new();
    let mut seq_stats = resolver::ResolutionStats {
        total: pendings.len(),
        ..Default::default()
    };
    for p in &pendings {
        match db.find_symbols_by_name(&p.callee_name) {
            Ok(candidates) => {
                if candidates.is_empty() {
                    seq_stats.no_candidates += 1;
                    continue;
                }
                if let Some(target) = resolver::select_best_candidate(
                    &candidates,
                    p,
                    &resolver::ParentReferenceContext::empty(),
                ) {
                    seq_resolved.push(resolver::build_resolved_relationship(p, target));
                    seq_stats.resolved += 1;
                } else {
                    seq_stats.no_valid_candidates += 1;
                }
            }
            Err(_) => seq_stats.lookup_errors += 1,
        }
    }

    // Results must be identical
    assert_eq!(batch_resolved.len(), seq_resolved.len());
    assert_eq!(batch_stats.resolved, seq_stats.resolved);
    assert_eq!(batch_stats.no_candidates, seq_stats.no_candidates);

    for (b, s) in batch_resolved.iter().zip(seq_resolved.iter()) {
        assert_eq!(b.from_symbol_id, s.from_symbol_id);
        assert_eq!(b.to_symbol_id, s.to_symbol_id);
        assert_eq!(b.kind, s.kind);
    }
}

// ─────────────────────────────────────────────────────────────────────
// Import-constrained disambiguation (resolve_batch + identifiers)
// ─────────────────────────────────────────────────────────────────────

/// Helper: symbol with a parent_id
fn child_sym(
    id: &str,
    name: &str,
    kind: SymbolKind,
    lang: &str,
    file_path: &str,
    parent_id: &str,
) -> Symbol {
    let mut s = sym(id, name, kind, lang, file_path);
    s.parent_id = Some(parent_id.to_string());
    s
}

/// Helper: create a minimal Identifier for testing
fn make_identifier(name: &str, kind: IdentifierKind, file_path: &str, lang: &str) -> Identifier {
    Identifier {
        id: format!("id_{}_{}", name, file_path.replace('/', "_")),
        name: name.to_string(),
        kind,
        language: lang.to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: name.len() as u32,
        start_byte: 0,
        end_byte: name.len() as u32,
        containing_symbol_id: None,
        target_symbol_id: None,
        confidence: 1.0,
        code_context: None,
    }
}

#[test]
fn test_resolve_batch_parent_reference_disambiguation() {
    // The LabHandbookV2 bug: two types have a method named "Success",
    // but the caller file only references AuthenticateResult.
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Set up files
    for (path, lang) in &[
        ("Auth/AuthenticateResult.cs", "csharp"),
        ("Api/ApiResponse.cs", "csharp"),
        ("Controllers/AuthController.cs", "csharp"),
    ] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: lang.to_string(),
            hash: "h".to_string(),
            size: 100,
            last_modified: 1000,
            last_indexed: 0,
            symbol_count: 2,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    // Two parent classes + two "Success" methods with different parents
    let symbols = vec![
        sym(
            "auth_result_class",
            "AuthenticateResult",
            SymbolKind::Class,
            "csharp",
            "Auth/AuthenticateResult.cs",
        ),
        child_sym(
            "auth_success",
            "Success",
            SymbolKind::Method,
            "csharp",
            "Auth/AuthenticateResult.cs",
            "auth_result_class",
        ),
        sym(
            "api_response_class",
            "ApiResponse",
            SymbolKind::Class,
            "csharp",
            "Api/ApiResponse.cs",
        ),
        child_sym(
            "api_success",
            "Success",
            SymbolKind::Method,
            "csharp",
            "Api/ApiResponse.cs",
            "api_response_class",
        ),
    ];
    db.store_symbols_transactional(&symbols).unwrap();

    // Caller file has a TypeUsage identifier for "AuthenticateResult"
    // (e.g., from a variable declaration or type annotation)
    let identifiers = vec![make_identifier(
        "AuthenticateResult",
        IdentifierKind::TypeUsage,
        "Controllers/AuthController.cs",
        "csharp",
    )];
    db.bulk_store_identifiers(&identifiers, "test_workspace")
        .unwrap();

    // Resolve: caller in AuthController.cs calls "Success"
    let pendings = vec![pending(
        "caller_method",
        "Success",
        "Controllers/AuthController.cs",
    )];

    let (resolved, stats) = resolver::resolve_batch(&pendings, &db);

    assert_eq!(stats.total, 1);
    assert_eq!(stats.resolved, 1, "Should resolve the pending relationship");
    assert_eq!(resolved.len(), 1);
    assert_eq!(
        resolved[0].to_symbol_id, "auth_success",
        "Should resolve to AuthenticateResult.Success, not ApiResponse.Success"
    );
}

#[test]
fn test_resolve_batch_no_identifiers_falls_back_gracefully() {
    // Same setup as above but WITHOUT identifiers — should still resolve
    // (just picks based on normal language/proximity heuristics)
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    for (path, lang) in &[
        ("Auth/AuthenticateResult.cs", "csharp"),
        ("Api/ApiResponse.cs", "csharp"),
        ("Controllers/AuthController.cs", "csharp"),
    ] {
        db.store_file_info(&FileInfo {
            path: path.to_string(),
            language: lang.to_string(),
            hash: "h".to_string(),
            size: 100,
            last_modified: 1000,
            last_indexed: 0,
            symbol_count: 2,
            line_count: 0,
            content: None,
        })
        .unwrap();
    }

    let symbols = vec![
        sym(
            "auth_result_class",
            "AuthenticateResult",
            SymbolKind::Class,
            "csharp",
            "Auth/AuthenticateResult.cs",
        ),
        child_sym(
            "auth_success",
            "Success",
            SymbolKind::Method,
            "csharp",
            "Auth/AuthenticateResult.cs",
            "auth_result_class",
        ),
        sym(
            "api_response_class",
            "ApiResponse",
            SymbolKind::Class,
            "csharp",
            "Api/ApiResponse.cs",
        ),
        child_sym(
            "api_success",
            "Success",
            SymbolKind::Method,
            "csharp",
            "Api/ApiResponse.cs",
            "api_response_class",
        ),
    ];
    db.store_symbols_transactional(&symbols).unwrap();

    // No identifiers stored — resolver should still work
    let pendings = vec![pending(
        "caller_method",
        "Success",
        "Controllers/AuthController.cs",
    )];

    let (resolved, stats) = resolver::resolve_batch(&pendings, &db);

    assert_eq!(stats.total, 1);
    assert_eq!(
        stats.resolved, 1,
        "Should still resolve even without identifier data"
    );
    assert_eq!(resolved.len(), 1);
    // Without import context, either candidate is acceptable — just verify resolution happened
}
