use std::collections::HashMap;

use anyhow::Result;
use tempfile::TempDir;

use crate::database::types::FileInfo;
use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind, Visibility};
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

    Ok(())
}
