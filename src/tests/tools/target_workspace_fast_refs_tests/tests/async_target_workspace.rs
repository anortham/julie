use super::*;

// =========================================================================
// Tests: the actual async function signature change compiles and works
// =========================================================================

/// Test that `find_references_in_target_workspace` accepts the new params.
/// This is a compile-time check + basic integration test using a real handler.
#[tokio::test(flavor = "multi_thread")]
async fn test_find_references_in_target_workspace_accepts_limit_and_kind() {
    use crate::handler::JulieServerHandler;
    use crate::tools::navigation::target_workspace;
    use std::fs;

    // Create primary workspace
    let primary_dir = TempDir::new().unwrap();
    let primary_path = primary_dir.path().to_path_buf();
    let primary_src = primary_path.join("src");
    fs::create_dir_all(&primary_src).unwrap();
    fs::write(primary_src.join("primary.rs"), "pub struct Primary {}").unwrap();

    // Create target workspace
    let reference_dir = TempDir::new().unwrap();
    let reference_path = reference_dir.path().to_path_buf();
    let reference_src = reference_path.join("src");
    fs::create_dir_all(&reference_src).unwrap();

    // Write a file with a function and some calls to it
    fs::write(
        reference_src.join("lib.rs"),
        r#"
pub fn compute(x: i32) -> i32 {
    x * 2
}

pub fn caller_one() {
    let result = compute(5);
}

pub fn caller_two() {
    compute(10);
}
"#,
    )
    .unwrap();

    // Initialize handler
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    // Compute target workspace ID and manually populate its database
    // (live indexing via manage_workspace would trigger expensive analysis;
    //  this test only needs specific symbols present to verify query behavior)
    let workspace = handler.get_workspace().await.unwrap().unwrap();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())
            .expect("Should compute workspace ID from target path");

    let ref_db_path = workspace.workspace_db_path(&workspace_id);
    fs::create_dir_all(ref_db_path.parent().unwrap()).unwrap();
    {
        let mut ref_db = SymbolDatabase::new(&ref_db_path).unwrap();
        ref_db
            .bulk_store_symbols(
                &[Symbol {
                    id: "compute_fn".to_string(),
                    name: "compute".to_string(),
                    kind: SymbolKind::Function,
                    language: "rust".to_string(),
                    file_path: reference_src.join("lib.rs").to_string_lossy().to_string(),
                    signature: Some("pub fn compute(x: i32) -> i32".to_string()),
                    start_line: 2,
                    start_column: 0,
                    end_line: 4,
                    end_column: 1,
                    start_byte: 1,
                    end_byte: 35,
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
                }],
                &workspace_id,
            )
            .unwrap();
    }

    // Call find_references_in_target_workspace with the new params.
    // This test validates that the function signature compiles with limit + reference_kind
    let result: Result<(Vec<Symbol>, Vec<Relationship>), anyhow::Error> =
        target_workspace::find_references_in_target_workspace(
            &handler,
            workspace_id,
            "compute",
            10,   // limit
            None, // reference_kind
        )
        .await;

    assert!(result.is_ok(), "should succeed: {:?}", result.err());
    let (defs, _refs) = result.unwrap();
    // We should find the "compute" definition
    assert!(
        !defs.is_empty(),
        "should find at least one definition for 'compute'"
    );

    // References may be empty if the tree-sitter extractor doesn't capture call
    // relationships in this simple case -- that's fine, we're testing the signature
    // and that it doesn't panic.
}

/// Regression coverage for the target-workspace path.
/// This checks the actual implementation, not the local test helper.
#[tokio::test(flavor = "multi_thread")]
async fn test_find_references_in_target_workspace_parity() {
    use crate::handler::JulieServerHandler;
    use crate::tools::navigation::target_workspace;
    use std::fs;

    let primary_dir = TempDir::new().unwrap();
    let primary_path = primary_dir.path().to_path_buf();
    let primary_src = primary_path.join("src");
    fs::create_dir_all(&primary_src).unwrap();
    fs::write(primary_src.join("primary.rs"), "pub struct Primary {}").unwrap();

    let target_dir = TempDir::new().unwrap();
    let target_path = target_dir.path().to_path_buf();
    let target_src = target_path.join("src");
    fs::create_dir_all(&target_src).unwrap();
    fs::write(target_src.join("engine.rs"), "pub struct Engine {}").unwrap();
    fs::write(target_src.join("pipeline.rs"), "pub struct Pipeline {}").unwrap();
    fs::write(target_src.join("engine_usage.rs"), "fn use_engine() {}").unwrap();
    fs::write(target_src.join("pipeline_usage.rs"), "fn use_pipeline() {}").unwrap();
    fs::write(target_src.join("imports.rs"), "use crate::Thing;").unwrap();

    let handler = JulieServerHandler::new_for_test().await.unwrap();
    handler
        .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
        .await
        .unwrap();

    let workspace = handler.get_workspace().await.unwrap().unwrap();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&target_path.to_string_lossy())
            .expect("Should compute workspace ID from target path");

    let ref_db_path = workspace.workspace_db_path(&workspace_id);
    fs::create_dir_all(ref_db_path.parent().unwrap()).unwrap();
    {
        let mut ref_db = SymbolDatabase::new(&ref_db_path).unwrap();
        ref_db
            .bulk_store_symbols(
                &[
                    Symbol {
                        id: "class-engine".to_string(),
                        name: "Engine".to_string(),
                        kind: SymbolKind::Class,
                        language: "rust".to_string(),
                        file_path: target_src.join("engine.rs").to_string_lossy().to_string(),
                        signature: Some("struct Engine".to_string()),
                        start_line: 1,
                        start_column: 0,
                        end_line: 1,
                        end_column: 20,
                        start_byte: 0,
                        end_byte: 20,
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
                    Symbol {
                        id: "method-engine-process".to_string(),
                        name: "process".to_string(),
                        kind: SymbolKind::Method,
                        language: "rust".to_string(),
                        file_path: target_src.join("engine.rs").to_string_lossy().to_string(),
                        signature: Some("pub fn process()".to_string()),
                        start_line: 5,
                        start_column: 0,
                        end_line: 9,
                        end_column: 1,
                        start_byte: 0,
                        end_byte: 40,
                        doc_comment: None,
                        visibility: None,
                        parent_id: Some("class-engine".to_string()),
                        metadata: None,
                        semantic_group: None,
                        confidence: None,
                        code_context: None,
                        content_type: None,
                        body_span: None,
                        body_hash: None,
                        annotations: Vec::new(),
                    },
                    Symbol {
                        id: "class-pipeline".to_string(),
                        name: "Pipeline".to_string(),
                        kind: SymbolKind::Class,
                        language: "rust".to_string(),
                        file_path: target_src.join("pipeline.rs").to_string_lossy().to_string(),
                        signature: Some("struct Pipeline".to_string()),
                        start_line: 1,
                        start_column: 0,
                        end_line: 1,
                        end_column: 22,
                        start_byte: 0,
                        end_byte: 22,
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
                    Symbol {
                        id: "method-pipeline-process".to_string(),
                        name: "process".to_string(),
                        kind: SymbolKind::Method,
                        language: "rust".to_string(),
                        file_path: target_src.join("pipeline.rs").to_string_lossy().to_string(),
                        signature: Some("pub fn process()".to_string()),
                        start_line: 5,
                        start_column: 0,
                        end_line: 9,
                        end_column: 1,
                        start_byte: 0,
                        end_byte: 40,
                        doc_comment: None,
                        visibility: None,
                        parent_id: Some("class-pipeline".to_string()),
                        metadata: None,
                        semantic_group: None,
                        confidence: None,
                        code_context: None,
                        content_type: None,
                        body_span: None,
                        body_hash: None,
                        annotations: Vec::new(),
                    },
                    Symbol {
                        id: "import-thing".to_string(),
                        name: "Thing".to_string(),
                        kind: SymbolKind::Import,
                        language: "rust".to_string(),
                        file_path: target_src.join("imports.rs").to_string_lossy().to_string(),
                        signature: Some("use crate::Thing".to_string()),
                        start_line: 1,
                        start_column: 0,
                        end_line: 1,
                        end_column: 18,
                        start_byte: 0,
                        end_byte: 18,
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
                ],
                &workspace_id,
            )
            .unwrap();
    }

    {
        let ref_db = SymbolDatabase::new(&ref_db_path).unwrap();
        ref_db
            .conn
            .execute("PRAGMA foreign_keys = OFF", [])
            .unwrap();
        insert_identifier_with_target(
            &ref_db,
            "process",
            "call",
            &target_src.join("engine_usage.rs").to_string_lossy(),
            10,
            None,
            Some("method-engine-process"),
            0.92,
        );
        insert_identifier_with_target(
            &ref_db,
            "process",
            "call",
            &target_src.join("engine_usage.rs").to_string_lossy(),
            10,
            None,
            Some("method-engine-process"),
            0.91,
        );
        insert_identifier_with_target(
            &ref_db,
            "process",
            "call",
            &target_src.join("pipeline_usage.rs").to_string_lossy(),
            12,
            None,
            Some("method-pipeline-process"),
            0.91,
        );
        insert_identifier(
            &ref_db,
            "Thing",
            "type_usage",
            &target_src.join("imports.rs").to_string_lossy(),
            2,
            None,
            0.73,
        );
    }

    let (defs_engine, refs_engine) = target_workspace::find_references_in_target_workspace(
        &handler,
        workspace_id.clone(),
        "Engine::process",
        10,
        None,
    )
    .await
    .expect("qualified lookup should succeed");

    assert_eq!(
        defs_engine.len(),
        1,
        "qualified lookup should return one child definition"
    );
    assert_eq!(defs_engine[0].id, "method-engine-process");
    assert_eq!(
        refs_engine.len(),
        1,
        "qualified lookup should keep only Engine::process identifier refs"
    );
    assert_eq!(
        refs_engine[0].file_path,
        target_src.join("engine_usage.rs").to_string_lossy(),
        "qualified lookup should not return the other parent's call"
    );

    let (defs_thing, refs_thing) = target_workspace::find_references_in_target_workspace(
        &handler,
        workspace_id,
        "Thing",
        10,
        None,
    )
    .await
    .expect("import lookup should succeed");

    assert!(
        defs_thing.is_empty(),
        "import symbols should not remain in definitions"
    );
    assert!(
        refs_thing
            .iter()
            .any(|r| r.kind == RelationshipKind::Imports),
        "import symbols should become import references"
    );
    assert!(
        refs_thing.iter().any(|r| r.kind == RelationshipKind::Uses),
        "type_usage identifiers should map to RelationshipKind::Uses"
    );
}
