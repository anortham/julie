use anyhow::Result;
use julie_context::{ToolContext, WorkspaceTarget};
use serde::de::DeserializeOwned;

use crate::tests::helpers::snapshot::snapshot_context_from_files;
use crate::tools::GetSymbolsTool;
use crate::tools::deep_dive::DeepDiveTool;

fn tool<T: DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).unwrap()
}

fn pages(result: &crate::mcp_compat::CallToolResult) -> &[serde_json::Value] {
    result.structured_content.as_ref().unwrap()["body_pages"]
        .as_array()
        .unwrap()
}

fn replay<T: DeserializeOwned>(continuation: &str, name: &str) -> T {
    let json = continuation
        .strip_prefix(&format!("next: {name} "))
        .unwrap();
    serde_json::from_str(json).unwrap()
}

#[tokio::test]
async fn full_symbol_body_is_bounded_and_reports_remaining_lines() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[(
        "src/body.rs",
        "pub fn bounded_body() {\n    let one = 1;\n    let two = 2;\n    let three = 3;\n}\n",
    )])?;
    let request: GetSymbolsTool = tool(serde_json::json!({
        "file_path": "src/body.rs",
        "target": "bounded_body",
        "mode": "full",
        "body_limit": 2
    }));

    let result = request.call_tool(&context).await?;
    let page = &pages(&result)[0];
    assert_eq!(page["body_offset"], 0);
    assert_eq!(page["returned_range"], serde_json::json!([0, 2]));
    assert_eq!(page["total_lines"], 5);
    assert_eq!(page["complete"], false);
    assert_eq!(page["text"], "pub fn bounded_body() {\n    let one = 1;\n");
    assert!(page["source_hash"].as_str().unwrap().len() >= 32);
    let continuation = page["continuation"].as_str().unwrap();
    let next: GetSymbolsTool = replay(continuation, "get_symbols");
    let next_json = serde_json::to_value(next)?;
    assert_eq!(next_json["body_offset"], 2);
    assert_eq!(next_json["body_limit"], 2);
    assert_eq!(next_json["source_hash"], page["source_hash"]);
    assert_eq!(next_json["workspace"], "snapshot-fixture");
    Ok(())
}

#[tokio::test]
async fn body_page_refuses_changed_source_hash() -> Result<()> {
    let (tree, context) = snapshot_context_from_files(&[(
        "src/body.rs",
        "pub fn changing_body() {\n    let old = true;\n}\n",
    )])?;
    let request: GetSymbolsTool = tool(serde_json::json!({
        "file_path": "src/body.rs",
        "target": "changing_body",
        "mode": "full",
        "body_limit": 1
    }));
    let first = request.call_tool(&context).await?;
    let next: GetSymbolsTool = replay(
        pages(&first)[0]["continuation"].as_str().unwrap(),
        "get_symbols",
    );
    std::fs::write(
        tree.path().join("src/body.rs"),
        "pub fn changing_body() {\n    let new = true;\n}\n",
    )?;

    let error = next.call_tool(&context).await.unwrap_err().to_string();
    assert!(error.contains("source changed"), "{error}");
    assert!(error.contains("body_offset=0"), "{error}");
    Ok(())
}

#[tokio::test]
async fn complete_value_declaration_requires_full_canonical_span() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[(
        "src/value.rs",
        "pub const MULTI_VALUE: &str = concat!(\n    \"café\",\n    \"雪\",\n);\n",
    )])?;
    let request: GetSymbolsTool = tool(serde_json::json!({
        "file_path": "src/value.rs",
        "target": "MULTI_VALUE",
        "mode": "full",
        "body_limit": 1
    }));

    let result = request.call_tool(&context).await?;
    let page = &pages(&result)[0];
    assert_eq!(page["complete"], false);
    assert_eq!(page["total_lines"], 4);
    assert_eq!(page["text"], "pub const MULTI_VALUE: &str = concat!(\n");
    assert!(page["continuation"].as_str().is_some());
    Ok(())
}

#[tokio::test]
async fn body_pages_reassemble_utf8_crlf_for_both_public_tools() -> Result<()> {
    let source = "pub const PAGED_VALUE: &str = concat!(\r\n    \"café\",\r\n    \"雪\",\r\n);\r\n";
    let (_tree, context) = snapshot_context_from_files(&[("src/value.rs", source)])?;

    let mut symbols: GetSymbolsTool = tool(serde_json::json!({
        "file_path": "src/value.rs",
        "target": "PAGED_VALUE",
        "mode": "full",
        "body_limit": 1
    }));
    let mut symbol_text = String::new();
    loop {
        let result = symbols.call_tool(&context).await?;
        let page = &pages(&result)[0];
        symbol_text.push_str(page["text"].as_str().unwrap());
        let Some(continuation) = page["continuation"].as_str() else {
            assert_eq!(page["complete"], false);
            assert_eq!(page["end_reached"], true);
            break;
        };
        symbols = replay(continuation, "get_symbols");
    }
    let canonical = source.strip_suffix("\r\n").unwrap();
    assert_eq!(symbol_text, canonical);

    let default_dive: DeepDiveTool = tool(serde_json::json!({
        "symbol": "PAGED_VALUE",
        "context_file": "src/value.rs",
        "depth": "full",
        "semantics": "off"
    }));
    let default_result = default_dive.call_tool(&context).await?;
    let default_text = &default_result.content[0].as_text().unwrap().text;
    assert!(default_text.contains("page_text=structuredContent.body_pages"));
    assert!(!default_text.contains("page_text=above-and-structuredContent"));

    let mut dive: DeepDiveTool = tool(serde_json::json!({
        "symbol": "PAGED_VALUE",
        "context_file": "src/value.rs",
        "depth": "full",
        "body_limit": 1,
        "semantics": "off"
    }));
    let mut dive_text = String::new();
    loop {
        let result = dive.call_tool(&context).await?;
        let page = &pages(&result)[0];
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .text
                .contains("PAGED_VALUE")
        );
        assert!(
            !result.content[0]
                .as_text()
                .unwrap()
                .text
                .contains("No symbol found")
        );
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .text
                .contains("page_text=above-and-structuredContent")
        );
        assert_eq!(
            result.structured_content.as_ref().unwrap()["symbol"],
            "PAGED_VALUE"
        );
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .text
                .contains(page["text"].as_str().unwrap())
        );
        dive_text.push_str(page["text"].as_str().unwrap());
        let Some(continuation) = page["continuation"].as_str() else {
            assert_eq!(page["complete"], false);
            assert_eq!(page["end_reached"], true);
            break;
        };
        dive = replay(continuation, "deep_dive");
    }
    assert_eq!(dive_text, canonical);
    Ok(())
}

#[tokio::test]
async fn stale_deep_dive_body_continuation_requires_restart() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[(
        "src/value.rs",
        "pub fn current_symbol() {\n    println!(\"current\");\n}\n",
    )])?;
    let request: DeepDiveTool = tool(serde_json::json!({
        "symbol": "obsolete-blob:42",
        "context_file": "src/value.rs",
        "depth": "full",
        "body_offset": 1,
        "body_limit": 1,
        "source_hash": "obsolete-blob",
        "semantics": "off"
    }));

    let error = request.call_tool(&context).await.unwrap_err().to_string();
    assert!(error.contains("source changed"), "{error}");
    assert!(error.contains("body_offset=0"), "{error}");
    Ok(())
}

#[tokio::test]
async fn stale_get_symbols_continuation_refuses_missing_target() -> Result<()> {
    let (_tree, context) = snapshot_context_from_files(&[(
        "src/value.rs",
        "pub fn renamed_symbol() {\n    println!(\"current\");\n}\n",
    )])?;
    let request: GetSymbolsTool = tool(serde_json::json!({
        "file_path": "src/value.rs",
        "target": "removed_symbol",
        "mode": "full",
        "body_offset": 1,
        "body_limit": 1,
        "source_hash": "obsolete-blob"
    }));

    let error = request.call_tool(&context).await.unwrap_err().to_string();
    assert!(error.contains("source changed"), "{error}");
    assert!(error.contains("body_offset=0"), "{error}");
    Ok(())
}

#[tokio::test]
async fn invalid_utf8_canonical_boundary_is_explicitly_unavailable() -> Result<()> {
    let source = "pub fn boundary() {\n    println!(\"café\");\n}\n";
    let (_tree, context) = snapshot_context_from_files(&[("src/body.rs", source)])?;
    let snapshot = context
        .snapshot(&WorkspaceTarget::Target("snapshot-fixture".into()))
        .await?;
    let id = snapshot.graph().symbols_in_path("src/body.rs")[0];
    let mut symbol = julie_tools::snapshot_rows::to_symbol(snapshot.graph(), id);
    symbol.extracted.start_byte = (source.find('é').unwrap() + 1) as u32;

    let page = crate::tools::symbols::body_extraction::body_page(&snapshot, &symbol, 0, 10, None)?;
    assert_eq!(page.status, "missing_canonical_span");
    assert!(!page.canonical_span_exact);
    assert!(!page.complete);
    assert!(page.text.is_empty());
    Ok(())
}

#[tokio::test]
async fn explicit_deep_dive_body_keeps_large_match_set_in_disambiguation_mode() -> Result<()> {
    let files = (0..6)
        .map(|index| {
            (
                format!("src/file_{index}.rs"),
                "pub fn repeated_name() {}\n".to_string(),
            )
        })
        .collect::<Vec<_>>();
    let borrowed = files
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect::<Vec<_>>();
    let (_tree, context) = snapshot_context_from_files(&borrowed)?;
    let request: DeepDiveTool = tool(serde_json::json!({
        "symbol": "repeated_name",
        "depth": "full",
        "body_limit": 1,
        "semantics": "off"
    }));

    let result = request.call_tool(&context).await?;
    assert!(
        result.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("Found 6 definitions")
    );
    assert!(pages(&result).is_empty());
    Ok(())
}
