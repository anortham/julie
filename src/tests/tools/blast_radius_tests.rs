use std::collections::HashMap;

use anyhow::Result;
use tempfile::TempDir;

use crate::database::types::FileInfo;
use crate::extractors::{
    Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind, Visibility,
};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::impact::BlastRadiusTool;
use crate::tools::spillover::SpilloverGetTool;

fn make_file(path: &str, hash: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: hash.to_string(),
        size: 256,
        last_modified: 1_700_000_000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 10,
        content: None,
    }
}

fn make_symbol(
    id: &str,
    name: &str,
    file_path: &str,
    visibility: Option<&str>,
    metadata: Option<HashMap<String, serde_json::Value>>,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        end_line: 3,
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 42,
        parent_id: None,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        visibility: visibility.map(|value| match value {
            "public" => Visibility::Public,
            "protected" => Visibility::Protected,
            "private" => Visibility::Private,
            other => panic!("unsupported visibility in test helper: {other}"),
        }),
        metadata,
        semantic_group: None,
        confidence: Some(1.0),
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    }
}

fn make_relationship(
    id: &str,
    from_symbol_id: &str,
    to_symbol_id: &str,
    kind: RelationshipKind,
    file_path: &str,
) -> Relationship {
    Relationship {
        id: id.to_string(),
        from_symbol_id: from_symbol_id.to_string(),
        to_symbol_id: to_symbol_id.to_string(),
        kind,
        file_path: file_path.to_string(),
        line_number: 1,
        confidence: 1.0,
        metadata: None,
    }
}

fn make_identifier(
    id: &str,
    name: &str,
    file_path: &str,
    containing_symbol_id: Option<&str>,
    target_symbol_id: Option<&str>,
    kind: IdentifierKind,
    confidence: f32,
) -> Identifier {
    Identifier {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: name.len() as u32,
        start_byte: 0,
        end_byte: name.len() as u32,
        containing_symbol_id: containing_symbol_id.map(str::to_string),
        target_symbol_id: target_symbol_id.map(str::to_string),
        confidence,
        code_context: None,
    }
}

fn extract_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|item| {
            serde_json::to_value(item).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(|text| text.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_spillover_handle(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("More available: spillover_handle=")
            .map(ToString::to_string)
    })
}

async fn setup_handler() -> Result<(TempDir, JulieServerHandler, String)> {
    let temp_dir = TempDir::new()?;
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    handler.initialize_workspace(None).await?;
    let workspace_id = handler
        .current_workspace_id()
        .expect("initialized workspace should bind a primary workspace id");
    Ok((temp_dir, handler, workspace_id))
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_ranks_direct_callers_and_uses_spillover() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let mut linkage = HashMap::new();
    linkage.insert(
        "test_linkage".to_string(),
        serde_json::json!({
            "test_count": 1,
            "best_tier": "thorough",
            "worst_tier": "thorough",
            "linked_tests": ["tests/request_tests.rs"],
            "evidence_sources": ["relationship"]
        }),
    );

    let files = vec![
        make_file("src/worker.rs", "hash_worker"),
        make_file("src/api.rs", "hash_api"),
        make_file("src/app.rs", "hash_app"),
    ];
    let symbols = vec![
        make_symbol("seed", "run_pipeline", "src/worker.rs", None, Some(linkage)),
        make_symbol(
            "direct",
            "handle_request",
            "src/api.rs",
            Some("public"),
            None,
        ),
        make_symbol("indirect", "app_entry", "src/app.rs", Some("public"), None),
    ];
    let relationships = vec![
        make_relationship(
            "rel_direct",
            "direct",
            "seed",
            RelationshipKind::Calls,
            "src/api.rs",
        ),
        make_relationship(
            "rel_indirect",
            "indirect",
            "direct",
            RelationshipKind::Calls,
            "src/app.rs",
        ),
    ];

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(&files, &symbols, &relationships, &[], &[], &workspace_id)?;
        guard.compute_reference_scores()?;
    }

    let result = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 2,
        limit: 1,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(
        text.contains("handle_request"),
        "first page should show direct caller first: {text}"
    );
    assert!(
        text.contains("tests/request_tests.rs"),
        "linked tests should be listed: {text}"
    );

    let spillover_handle =
        extract_spillover_handle(&text).expect("first page should emit spillover handle");
    let spillover_text = extract_text(
        &SpilloverGetTool {
            spillover_handle,
            limit: Some(5),
            format: Some("readable".to_string()),
        }
        .call_tool(&handler)
        .await?,
    );

    assert!(
        spillover_text.contains("app_entry"),
        "spillover page should contain indirect caller: {spillover_text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_likely_tests_include_resolved_refs_to_impacted_symbols() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let files = vec![
        make_file("src/service.rs", "hash_service"),
        make_file("src/api.rs", "hash_api"),
        make_file("src/helper.rs", "hash_helper"),
        make_file("tests/request_flow.rs", "hash_request_flow"),
    ];
    let symbols = vec![
        make_symbol("seed", "run_service", "src/service.rs", None, None),
        make_symbol(
            "impact",
            "handle_request",
            "src/api.rs",
            Some("public"),
            None,
        ),
        make_symbol("helper", "build_helper", "src/helper.rs", None, None),
        make_symbol(
            "test_symbol",
            "test_request_flow",
            "tests/request_flow.rs",
            None,
            None,
        ),
    ];
    let relationships = vec![make_relationship(
        "impact_calls_seed",
        "impact",
        "seed",
        RelationshipKind::Calls,
        "src/api.rs",
    )];
    let identifiers = vec![
        make_identifier(
            "seed_non_test_ref",
            "run_service",
            "src/helper.rs",
            Some("helper"),
            Some("seed"),
            IdentifierKind::Call,
            0.99,
        ),
        make_identifier(
            "seed_self_ref",
            "run_service",
            "src/service.rs",
            Some("seed"),
            Some("seed"),
            IdentifierKind::Call,
            1.0,
        ),
        make_identifier(
            "impact_test_ref",
            "handle_request",
            "tests/request_flow.rs",
            Some("test_symbol"),
            Some("impact"),
            IdentifierKind::Call,
            0.98,
        ),
    ];

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &files,
            &symbols,
            &relationships,
            &identifiers,
            &[],
            &workspace_id,
        )?;
        guard.compute_reference_scores()?;
    }

    let result = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 1,
        limit: 5,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(
        text.contains("handle_request"),
        "impacted symbol should appear in blast radius: {text}"
    );
    assert!(
        text.contains("tests/request_flow.rs"),
        "resolved identifier refs to impacted symbols should produce likely tests: {text}"
    );
    assert!(
        text.contains("test_request_flow"),
        "related test symbol should be surfaced with the likely path: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_likely_test_path_overflow_is_retrievable() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let linked_test_paths: Vec<String> = (0..12)
        .map(|index| format!("tests/generated/test_{index}.rs"))
        .collect();
    let mut linkage = HashMap::new();
    linkage.insert(
        "test_linkage".to_string(),
        serde_json::json!({
            "test_count": linked_test_paths.len(),
            "best_tier": "basic",
            "worst_tier": "basic",
            "linked_test_paths": linked_test_paths,
            "evidence_sources": ["metadata"]
        }),
    );

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &[make_file("src/worker.rs", "hash_worker")],
            &[make_symbol(
                "seed",
                "run_pipeline",
                "src/worker.rs",
                None,
                Some(linkage),
            )],
            &[],
            &[],
            &[],
            &workspace_id,
        )?;
        guard.compute_reference_scores()?;
    }

    let result = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 1,
        limit: 5,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(
        text.contains("tests/generated/test_9.rs"),
        "visible likely tests should include the capped prefix: {text}"
    );
    assert!(
        !text.contains("tests/generated/test_10.rs"),
        "overflow likely tests should stay out of the first page: {text}"
    );

    let spillover_handle =
        extract_spillover_handle(&text).expect("likely-test overflow should emit spillover handle");
    let spillover_text = extract_text(
        &SpilloverGetTool {
            spillover_handle,
            limit: Some(10),
            format: Some("readable".to_string()),
        }
        .call_tool(&handler)
        .await?,
    );

    assert!(
        spillover_text.contains("Blast radius likely-test paths"),
        "spillover title should identify likely-test paths: {spillover_text}"
    );
    assert!(
        spillover_text.contains("tests/generated/test_10.rs")
            && spillover_text.contains("tests/generated/test_11.rs"),
        "spillover page should include hidden likely-test paths: {spillover_text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_related_test_symbol_overflow_is_retrievable() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let linked_tests: Vec<String> = (0..12)
        .map(|index| format!("test_generated_case_{index}"))
        .collect();
    let mut linkage = HashMap::new();
    linkage.insert(
        "test_linkage".to_string(),
        serde_json::json!({
            "test_count": linked_tests.len(),
            "best_tier": "basic",
            "worst_tier": "basic",
            "linked_tests": linked_tests,
            "evidence_sources": ["metadata"]
        }),
    );

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &[make_file("src/worker.rs", "hash_worker")],
            &[make_symbol(
                "seed",
                "run_pipeline",
                "src/worker.rs",
                None,
                Some(linkage),
            )],
            &[],
            &[],
            &[],
            &workspace_id,
        )?;
        guard.compute_reference_scores()?;
    }

    let result = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 1,
        limit: 5,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(
        text.contains("test_generated_case_9"),
        "visible related test symbols should include the capped prefix: {text}"
    );
    assert!(
        !text.contains("test_generated_case_10"),
        "overflow related test symbols should stay out of the first page: {text}"
    );

    let spillover_handle = extract_spillover_handle(&text)
        .expect("related-test-symbol overflow should emit spillover handle");
    let spillover_text = extract_text(
        &SpilloverGetTool {
            spillover_handle,
            limit: Some(10),
            format: Some("readable".to_string()),
        }
        .call_tool(&handler)
        .await?,
    );

    assert!(
        spillover_text.contains("Blast radius related test symbols"),
        "spillover title should identify related test symbols: {spillover_text}"
    );
    assert!(
        spillover_text.contains("test_generated_case_10")
            && spillover_text.contains("test_generated_case_11"),
        "spillover page should include hidden related test symbols: {spillover_text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_reports_deleted_files_from_revision_range() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;
    let db = handler.primary_database().await?;

    let first_revision = {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.incremental_update_atomic(
            &[],
            &[make_file("src/legacy.rs", "hash_legacy_v1")],
            &[make_symbol(
                "legacy",
                "legacy_fn",
                "src/legacy.rs",
                None,
                None,
            )],
            &[],
            &[],
            &[],
            &workspace_id,
        )?;
        guard
            .get_current_canonical_revision(&workspace_id)?
            .expect("first write should record revision")
    };

    let second_revision = {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard
            .delete_orphaned_files_atomic(&workspace_id, &["src/legacy.rs".to_string()])?
            .expect("delete should record revision")
    };

    let result = BlastRadiusTool {
        symbol_ids: vec![],
        file_paths: vec![],
        from_revision: Some(first_revision),
        to_revision: Some(second_revision),
        max_depth: 2,
        limit: 5,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);
    assert!(
        text.contains("Deleted files"),
        "deleted-file section should be present: {text}"
    );
    assert!(
        text.contains("src/legacy.rs"),
        "deleted file should be reported: {text}"
    );
    assert!(
        text.contains("deleted-file impact is path-only")
            && text.contains("historical callers are unavailable"),
        "deletion-only revision ranges should state the impact limit: {text}"
    );

    Ok(())
}
