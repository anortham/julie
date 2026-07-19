use std::collections::HashMap;

use anyhow::Result;
use julie_context::WorkspaceTarget;
use julie_core::database::SymbolDatabase;
use julie_core::database::bulk::atomic::{AtomicPersistenceMetadata, CanonicalWriteSet};
use julie_extractors::base::StructuralFact;
use julie_test_support::FakeToolContext;
use julie_test_support::db::file_info_builder;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tools::patterns::{PatternsFormat, PatternsGroupBy, PatternsOperation, PatternsTool};

fn structural_fact(
    id: &str,
    pattern_id: &str,
    capture_name: &str,
    file_path: &str,
    language: &str,
    start_line: u32,
    metadata: serde_json::Value,
) -> StructuralFact {
    StructuralFact {
        id: id.into(),
        file_path: file_path.into(),
        language: language.into(),
        pattern_id: pattern_id.into(),
        capture_name: capture_name.into(),
        node_kind: "call_expression".into(),
        containing_symbol_id: None,
        start_line,
        start_column: 0,
        end_line: start_line,
        end_column: 12,
        start_byte: start_line * 10,
        end_byte: start_line * 10 + 12,
        confidence: 0.95,
        metadata: serde_json::from_value::<HashMap<String, serde_json::Value>>(metadata).ok(),
    }
}

fn seeded_context() -> Result<(TempDir, FakeToolContext)> {
    let temp = TempDir::new()?;
    let db_path = temp.path().join("patterns.db");
    let mut db = SymbolDatabase::new(&db_path)?;
    let files = vec![
        file_info_builder("src/client.rs").language("rust").build(),
        file_info_builder("src/Controller.php")
            .language("php")
            .build(),
        file_info_builder("tests/client.rs")
            .language("rust")
            .build(),
    ];
    let facts = vec![
        structural_fact(
            "fact-1",
            "http.client_request.v1",
            "request",
            "src/client.rs",
            "rust",
            3,
            serde_json::json!({"client": "reqwest", "method": "GET"}),
        ),
        structural_fact(
            "fact-2",
            "symfony.route.v1",
            "route",
            "src/Controller.php",
            "php",
            8,
            serde_json::json!({"method": "POST"}),
        ),
        structural_fact(
            "fact-3",
            "http.client_request.v1",
            "request",
            "tests/client.rs",
            "rust",
            5,
            serde_json::json!({"client": "reqwest", "method": "POST"}),
        ),
    ];
    let write_set = CanonicalWriteSet {
        files: &files,
        structural_facts: &facts,
        ..Default::default()
    };
    db.incremental_update_atomic_with_metadata(
        &[],
        &write_set,
        "patterns-test",
        AtomicPersistenceMetadata::default(),
    )?;
    drop(db);

    let context = FakeToolContext::new()
        .with_workspace_id("patterns-test")
        .with_primary_root(temp.path())
        .with_primary_db_path(&db_path);
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
        language: Some("rust".into()),
        where_filter: Some("client=reqwest;method=GET".into()),
        limit: 1,
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let searched_text = call_tool_result_text(&searched);
    assert!(searched_text.contains("\"fact-1\""));
    assert!(!searched_text.contains("\"fact-2\""));
    assert!(!searched_text.contains("\"fact-3\""));

    let no_match = PatternsTool {
        operation: PatternsOperation::Search,
        query: Some("not_an_observed_pattern".into()),
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&context)
    .await?;
    let no_match_text = call_tool_result_text(&no_match);
    assert!(!no_match_text.contains("\"fact-1\""));
    assert!(!no_match_text.contains("\"fact-2\""));
    assert!(!no_match_text.contains("\"fact-3\""));

    let exact_compact = PatternsTool {
        operation: PatternsOperation::Search,
        pattern_id: Some("http.client_request.v1".into()),
        where_filter: Some("method=GET".into()),
        workspace: Some("target-workspace".into()),
        format: PatternsFormat::Compact,
        ..Default::default()
    }
    .call_tool(
        &FakeToolContext::new()
            .with_primary_db_path(_temp.path().join("patterns.db"))
            .with_resolved_target(WorkspaceTarget::Target("target-workspace".into())),
    )
    .await?;
    let compact_text = call_tool_result_text(&exact_compact);
    assert!(compact_text.contains("src/client.rs:3"));
    assert!(compact_text.contains("http.client_request.v1"));
    assert!(compact_text.contains("request"));
    assert!(compact_text.contains("method=GET"));

    let summary = PatternsTool {
        operation: PatternsOperation::Summary,
        group_by: PatternsGroupBy::Directory,
        facet: Some("method".into()),
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
