use std::fs;
use std::path::Path;

use anyhow::Result;
use julie_context::WorkspaceTarget;
use julie_test_support::FakeToolContext;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::patterns::{PatternsFormat, PatternsGroupBy, PatternsOperation, PatternsTool};

const GET_CLIENT: &str = "export async function load() {\n  return fetch(\"/api/users\");\n}\n";
const POST_CLIENT: &str =
    "export async function save() {\n  return fetch(\"/api/users\", { method: \"POST\" });\n}\n";
const SYMFONY_CONTROLLER: &str = r#"<?php
namespace App\Controller;

use Symfony\Component\Routing\Attribute\Route;

class UserController
{
    #[Route('/users', methods: ['POST'])]
    public function create(): void {}
}
"#;

fn write_tree(root: &Path, files: &[(&str, &str)]) -> Result<()> {
    for (path, source) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, source)?;
    }
    Ok(())
}

fn seeded_context() -> Result<(TempDir, FakeToolContext)> {
    let temp = TempDir::new()?;
    write_tree(
        temp.path(),
        &[
            ("src/client.ts", GET_CLIENT),
            ("src/Controller.php", SYMFONY_CONTROLLER),
            ("tests/client.ts", POST_CLIENT),
        ],
    )?;
    let context = snapshot_context(temp.path())?;
    Ok((temp, context))
}

#[tokio::test]
async fn patterns_lists_searches_summarizes_and_filters_metadata() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let listed = PatternsTool {
        operation: PatternsOperation::List,
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let listed_text = call_tool_result_text(&listed);
    assert!(listed_text.contains("\"http.client_request.v1\""));
    assert!(listed_text.contains("\"symfony.route.v1\""));

    let searched = PatternsTool {
        operation: PatternsOperation::Search,
        query: Some("client_request".into()),
        path: Some("src/**".into()),
        language: Some("typescript".into()),
        where_filter: Some("client=fetch;verb=GET".into()),
        limit: 1,
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let searched_text = call_tool_result_text(&searched);
    assert!(searched_text.contains("\"src/client.ts\""));
    assert!(!searched_text.contains("\"src/Controller.php\""));
    assert!(!searched_text.contains("\"tests/client.ts\""));

    let no_match = PatternsTool {
        operation: PatternsOperation::Search,
        query: Some("not_an_observed_pattern".into()),
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let no_match_text = call_tool_result_text(&no_match);
    assert!(!no_match_text.contains("\"src/client.ts\""));
    assert!(!no_match_text.contains("\"src/Controller.php\""));
    assert!(!no_match_text.contains("\"tests/client.ts\""));

    let exact_compact = PatternsTool {
        operation: PatternsOperation::Search,
        pattern_id: Some("http.client_request.v1".into()),
        where_filter: Some("verb=GET".into()),
        workspace: Some("target-workspace".into()),
        format: PatternsFormat::Compact,
        ..Default::default()
    }
    .call_tool(
        &snapshot_context(_temp.path())?
            .with_resolved_target(WorkspaceTarget::Target("target-workspace".into())),
    )
    .await?;
    let compact_text = call_tool_result_text(&exact_compact);
    assert!(compact_text.contains("src/client.ts:2"));
    assert!(compact_text.contains("http.client_request.v1"));
    assert!(compact_text.contains("request"));
    assert!(compact_text.contains("verb=GET"));

    let summary = PatternsTool {
        operation: PatternsOperation::Summary,
        group_by: PatternsGroupBy::Directory,
        facet: Some("verb".into()),
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let summary_text = call_tool_result_text(&summary);
    assert!(summary_text.contains("\"directory\":\"src\""));
    assert!(summary_text.contains("\"facet_value\":\"GET\""));
    assert!(summary_text.contains("\"facet_value\":\"POST\""));

    Ok(())
}

#[tokio::test]
async fn patterns_respects_target_workspace() -> Result<()> {
    let temp = TempDir::new()?;
    write_tree(temp.path(), &[("target-workspace.ts", POST_CLIENT)])?;
    let context = snapshot_context(temp.path())?
        .with_workspace_id("primary-workspace")
        .with_resolved_target(WorkspaceTarget::Target("target-workspace".into()));

    let response = PatternsTool {
        operation: PatternsOperation::Search,
        query: Some("client_request".into()),
        workspace: Some("target-workspace".into()),
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let response_text = call_tool_result_text(&response);

    assert!(response_text.contains("\"target-workspace.ts\""));
    assert!(!response_text.contains("\"primary-workspace.ts\""));
    Ok(())
}

#[tokio::test]
async fn patterns_rejects_invalid_parameters() -> Result<()> {
    let (_temp, context) = seeded_context()?;

    let missing_selector = PatternsTool {
        operation: PatternsOperation::Search,
        ..Default::default()
    }
    .call_tool(&context)
    .await
    .unwrap_err();
    assert_eq!(
        missing_selector.to_string(),
        "patterns search requires pattern_id or query"
    );

    let malformed_where = PatternsTool {
        operation: PatternsOperation::Search,
        pattern_id: Some("http.client_request.v1".into()),
        where_filter: Some("client".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await
    .unwrap_err();
    assert_eq!(
        malformed_where.to_string(),
        "where filters must use key=value"
    );

    let empty_where_value = PatternsTool {
        operation: PatternsOperation::Search,
        query: Some("client".into()),
        where_filter: Some("client=".into()),
        ..Default::default()
    }
    .call_tool(&context)
    .await
    .unwrap_err();
    assert_eq!(
        empty_where_value.to_string(),
        "where filters must use non-empty key=value"
    );

    let unknown_operation = serde_json::from_value::<PatternsTool>(serde_json::json!({
        "operation": "inspect"
    }))
    .unwrap_err();
    assert!(
        unknown_operation
            .to_string()
            .contains("unknown variant `inspect`")
    );

    let unknown_group = serde_json::from_value::<PatternsTool>(serde_json::json!({
        "group_by": "package"
    }))
    .unwrap_err();
    assert!(
        unknown_group
            .to_string()
            .contains("unknown variant `package`")
    );

    let unknown_format = serde_json::from_value::<PatternsTool>(serde_json::json!({
        "format": "yaml"
    }))
    .unwrap_err();
    assert!(
        unknown_format
            .to_string()
            .contains("unknown variant `yaml`")
    );

    Ok(())
}
