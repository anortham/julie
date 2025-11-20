// TDD Test for Workspace Isolation Bug in trace_call_path
//
// BUG: semantic_neighbors() always uses handler.get_workspace() which returns
// the PRIMARY workspace, completely ignoring the workspace parameter.
//
// This test demonstrates that trace_call_path violates workspace isolation
// by returning semantic matches from the primary workspace when searching
// a reference workspace.

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::base::Visibility;
use crate::extractors::{Symbol, SymbolKind};
use crate::tools::trace_call_path::TraceCallPathTool;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

fn make_symbol(id: &str, name: &str, language: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Class,
        language: language.to_string(),
        file_path: file_path.to_string(),
        signature: None,
        start_line: 1,
        start_column: 0,
        end_line: 10,
        end_column: 1,
        start_byte: 0,
        end_byte: 100,
        doc_comment: Some("A test class".to_string()),
        visibility: Some(Visibility::Public),
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some("class TestClass { }".to_string()),
        content_type: None,
    }
}

/// RED PHASE: This test should FAIL because semantic_neighbors uses wrong workspace
///
/// Test scenario:
/// 1. Primary workspace has symbol "PaymentService" at /primary/payment.ts
/// 2. Reference workspace has symbol "UserService" at /reference/user.ts
/// 3. When searching for "UserService" in reference workspace with semantic search
/// 4. Results should ONLY include symbols from /reference/* paths
/// 5. Results should NEVER include symbols from /primary/* paths
///
/// Current behavior (BUG):
/// - semantic_neighbors() calls handler.get_workspace() which returns PRIMARY
/// - HNSW search happens on PRIMARY workspace vector store
/// - Results leak from primary workspace into reference workspace search
///
/// Expected behavior (AFTER FIX):
/// - semantic_neighbors() should use the workspace parameter
/// - HNSW search should happen on the REFERENCE workspace vector store
/// - Results strictly isolated to the target workspace
#[tokio::test]
#[ignore] // Requires full workspace setup with vector stores - complex integration test
async fn test_semantic_search_respects_workspace_isolation() {
    // This is a placeholder for a full integration test
    // The real bug was discovered manually during testing:
    // - Searched MyraNext workspace (reference) for "InstitutionalProposalProject"
    // - Got results from Julie workspace (primary): /Users/murphy/source/julie/...
    // - This violates fundamental workspace isolation principle

    // TODO: Implement full integration test when we have test infrastructure for:
    // 1. Creating multiple workspaces with vector stores
    // 2. Loading HNSW indexes
    // 3. Running semantic search across workspace boundaries

    panic!("Test not yet implemented - see manual reproduction in conversation");
}

/// Unit test for find_cross_language_callers workspace isolation
///
/// This tests the non-semantic code path to ensure workspace isolation
/// works correctly for naming variant matching (which doesn't use HNSW).
#[tokio::test]
async fn test_naming_variants_respect_workspace_database() {
    // Setup: Create TWO separate databases simulating primary + reference workspaces
    let temp_primary = tempdir().expect("primary tempdir");
    let temp_reference = tempdir().expect("reference tempdir");

    let primary_db_path = temp_primary.path().join("primary.db");
    let reference_db_path = temp_reference.path().join("reference.db");

    let primary_db = Arc::new(Mutex::new(
        SymbolDatabase::new(&primary_db_path).expect("primary db"),
    ));
    let reference_db = Arc::new(Mutex::new(
        SymbolDatabase::new(&reference_db_path).expect("reference db"),
    ));

    // Primary workspace has: process_payment (Python) and ProcessPayment (C#)
    let primary_symbol = make_symbol(
        "primary_1",
        "process_payment",
        "python",
        "/primary/workspace/payment.py",
    );

    let primary_variant = make_symbol(
        "primary_2",
        "ProcessPayment",
        "csharp",
        "/primary/workspace/Payment.cs",
    );

    // Reference workspace has: process_payment (TypeScript) and ProcessPayment (Java)
    let reference_symbol = make_symbol(
        "ref_1",
        "process_payment",
        "typescript",
        "/reference/workspace/payment.ts",
    );

    let reference_variant = make_symbol(
        "ref_2",
        "ProcessPayment",
        "java",
        "/reference/workspace/Payment.java",
    );

    // Store symbols in respective databases
    {
        let mut primary_guard = primary_db.lock().unwrap();

        let file1 = FileInfo {
            path: primary_symbol.file_path.clone(),
            language: primary_symbol.language.clone(),
            hash: "hash1".to_string(),
            size: 100,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 1,
            content: Some("".to_string()),
        };

        let file2 = FileInfo {
            path: primary_variant.file_path.clone(),
            language: primary_variant.language.clone(),
            hash: "hash2".to_string(),
            size: 100,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 1,
            content: Some("".to_string()),
        };

        primary_guard.store_file_info(&file1).expect("store file1");
        primary_guard.store_file_info(&file2).expect("store file2");
        primary_guard
            .store_symbols_transactional(&[primary_symbol.clone(), primary_variant.clone()])
            .expect("store primary symbols");
    }

    {
        let mut reference_guard = reference_db.lock().unwrap();

        let file3 = FileInfo {
            path: reference_symbol.file_path.clone(),
            language: reference_symbol.language.clone(),
            hash: "hash3".to_string(),
            size: 100,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 1,
            content: Some("".to_string()),
        };

        let file4 = FileInfo {
            path: reference_variant.file_path.clone(),
            language: reference_variant.language.clone(),
            hash: "hash4".to_string(),
            size: 100,
            last_modified: 0,
            last_indexed: 0,
            symbol_count: 1,
            content: Some("".to_string()),
        };

        reference_guard
            .store_file_info(&file3)
            .expect("store file3");
        reference_guard
            .store_file_info(&file4)
            .expect("store file4");
        reference_guard
            .store_symbols_transactional(&[reference_symbol.clone(), reference_variant.clone()])
            .expect("store reference symbols");
    }

    // Test: Search for cross-language callers in REFERENCE workspace
    let tool = TraceCallPathTool {
        symbol: "process_payment".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        context_file: None,
        workspace: Some("reference_workspace_123".to_string()), // Reference workspace
        output_format: Some("json".to_string()),
    };

    // Call find_cross_language_callers with REFERENCE database
    let callers = tool
        .find_cross_language_callers(&reference_db, &reference_symbol)
        .await
        .expect("find callers");

    // VERIFY: Results should ONLY come from reference workspace
    for caller in &callers {
        assert!(
            caller.file_path.starts_with("/reference/"),
            "WORKSPACE ISOLATION VIOLATED: Found symbol from wrong workspace: {} (should only have /reference/* paths)",
            caller.file_path
        );
    }

    // Should find ProcessPayment (Java) from reference workspace
    assert_eq!(
        callers.len(),
        1,
        "Expected 1 cross-language caller in reference workspace"
    );
    assert_eq!(callers[0].name, "ProcessPayment");
    assert_eq!(callers[0].language, "java");
    assert!(callers[0].file_path.contains("/reference/"));

    // Verify it's NOT the C# variant from primary workspace
    assert_ne!(
        callers[0].language, "csharp",
        "Should not find primary workspace symbols"
    );
}

/// Documentation of the architectural bug
///
/// The bug is in src/tools/trace_call_path.rs:812-815:
///
/// ```rust
/// let workspace = match handler.get_workspace().await? {
///     Some(ws) => ws,
///     None => return Ok(vec![]),
/// };
/// ```
///
/// This ALWAYS gets the PRIMARY workspace from the handler, ignoring the
/// workspace parameter passed to call_tool(). The fix requires:
///
/// 1. Load the correct workspace's vector store based on self.workspace parameter
/// 2. Pass that vector store to semantic_neighbors
/// 3. Update semantic_neighbors signature to accept vector store parameter
/// 4. Update both callers of semantic_neighbors
#[test]
fn document_semantic_neighbors_architectural_bug() {
    // This test documents the root cause for future reference
    // See conversation for full details of the bug discovery
}
