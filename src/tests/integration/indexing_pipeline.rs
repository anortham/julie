use std::fs;
use std::sync::atomic::Ordering;

use anyhow::Result;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::workspace::JulieWorkspace;
use crate::mcp_compat::CallToolResult;
use crate::tools::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::pipeline::run_indexing_pipeline;
use crate::tools::workspace::indexing::route::IndexRoute;
use crate::tools::workspace::indexing::state::{
    IndexingOperation, IndexingStage,
};
use crate::workspace::mutation_gate::acquire_gate;

fn workspace_tool() -> ManageWorkspaceTool {
    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    }
}

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn fast_search_text(
    handler: &JulieServerHandler,
    query: &str,
    exclude_tests: Option<bool>,
) -> Result<String> {
    let tool = FastSearchTool {
        query: query.to_string(),
        limit: 10,
        context_lines: Some(3),
        workspace: Some("primary".to_string()),
        exclude_tests,
        ..Default::default()
    };

    let result = tool.call_tool(handler).await?;
    Ok(extract_text_from_result(&result))
}

#[test]
fn test_indexing_pipeline_project_batch_does_not_clone_large_batch_vectors() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/tools/workspace/indexing/pipeline.rs"
    ))
    .expect("read indexing pipeline source");

    for forbidden in [
        "batch.all_symbols.clone()",
        "batch.all_file_infos.clone()",
        "batch.files_to_clean.clone()",
    ] {
        assert!(
            !source.contains(forbidden),
            "project_batch must move large extracted batch vectors into projection, not clone `{forbidden}`"
        );
    }
}

async fn test_handler_and_route(
    temp_dir: &TempDir,
) -> Result<(JulieServerHandler, std::path::PathBuf, IndexRoute)> {
    let workspace = JulieWorkspace::initialize(temp_dir.path().to_path_buf()).await?;
    let workspace_root = workspace.root.clone();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&workspace_root.to_string_lossy())?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        std::sync::Arc::new(workspace),
        workspace_root.clone(),
        None,
        Some(workspace_id),
        None,
    )
    .await?;

    let route = IndexRoute::for_workspace_path(&handler, &workspace_root)
        .await
        .map_err(anyhow::Error::new)?;

    Ok((handler, workspace_root, route))
}

async fn symbol_count(handler: &JulieServerHandler, _route: &IndexRoute) -> Result<i64> {
    let workspace = handler.get_workspace().await?.expect("workspace");
    Ok(workspace.store.status().graph.symbols as i64)
}

async fn symbol_count_for_file(
    handler: &JulieServerHandler,
    _route: &IndexRoute,
    file_path: &str,
) -> Result<i64> {
    let workspace = handler.get_workspace().await?.expect("workspace");
    let snapshot = workspace.store.current();
    Ok(snapshot.graph().symbols_in_path(file_path).len() as i64)
}

async fn annotation_rows(
    handler: &JulieServerHandler,
    _route: &IndexRoute,
) -> Result<Vec<(String, String, String)>> {
    let workspace = handler.get_workspace().await?.expect("workspace");
    let snapshot = workspace.store.current();
    let graph = snapshot.graph();
    let mut rows = Vec::new();
    for i in 0..graph.len() {
        let symbol = graph.symbol(julie_index::graph::SymbolId(i as u32));
        for annotation in &symbol.annotations {
            rows.push((
                symbol.name.clone(),
                symbol.path.clone(),
                annotation.annotation_key.clone(),
            ));
        }
    }
    rows.sort();
    Ok(rows)
}

fn has_annotation(rows: &[(String, String, String)], symbol_name: &str, key: &str) -> bool {
    rows.iter()
        .any(|(name, _path, annotation_key)| name == symbol_name && annotation_key == key)
}

async fn parent_name_for_symbol(
    handler: &JulieServerHandler,
    _route: &IndexRoute,
    symbol_name: &str,
) -> Result<Option<String>> {
    let workspace = handler.get_workspace().await?.expect("workspace");
    let snapshot = workspace.store.current();
    let graph = snapshot.graph();
    for i in 0..graph.len() {
        let symbol = graph.symbol(julie_index::graph::SymbolId(i as u32));
        if symbol.name != symbol_name {
            continue;
        }
        let Some(parent_ord) = symbol.parent_ordinal else {
            return Ok(None);
        };
        for id in graph.symbols_in_path(&symbol.path) {
            let parent = graph.symbol(*id);
            if parent.ordinal == parent_ord {
                return Ok(Some(parent.name.clone()));
            }
        }
        return Ok(None);
    }
    Ok(None)
}

fn expected_stage_history() -> Vec<IndexingStage> {
    vec![
        IndexingStage::Queued,
        IndexingStage::Grouped,
        IndexingStage::Extracting,
        IndexingStage::Persisting,
        IndexingStage::Resolving,
        IndexingStage::Projecting,
        IndexingStage::Analyzing,
        IndexingStage::Completed,
    ]
}

#[tokio::test]
async fn test_indexing_pipeline_reports_stage_history_for_parser_backed_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    fs::write(temp_dir.path().join("lib.rs"), "fn parser_backed() {}\n")?;

    let (handler, workspace_root, route) = test_handler_and_route(&temp_dir).await?;
    let result = run_indexing_pipeline(
        &workspace_tool(),
        &handler,
        vec![workspace_root.join("lib.rs")],
        &route,
        IndexingOperation::Incremental,
        &acquire_gate(&route.workspace_id).await,
    )
    .await?;

    assert_eq!(
        result.state.stage_history,
        expected_stage_history(),
        "parser-backed files should traverse the full indexing pipeline"
    );
    assert_eq!(result.files_processed, 1, "one file should be processed");
    assert_eq!(result.state.parsed_file_count(), 1);
    assert_eq!(result.state.text_only_file_count(), 0);
    assert_eq!(result.state.repair_file_count(), 0);
    assert!(
        handler.indexing_status.search_ready.load(Ordering::Acquire),
        "successful pipeline runs should publish search readiness"
    );
    Ok(())
}

#[tokio::test]
async fn test_indexing_pipeline_persists_annotations_and_searches_normalized_markers() -> Result<()>
{
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();
    let src_dir = root.join("src");
    let java_dir = src_dir.join("main/java/example");
    fs::create_dir_all(&java_dir)?;

    let python_file = src_dir.join("app.py");
    let java_file = java_dir.join("UserController.java");
    let csharp_file = src_dir.join("UserBehavior.cs");
    let cpp_file = src_dir.join("native_marker.cpp");

    fs::write(
        &python_file,
        r#"
from flask import Flask

app = Flask(__name__)

@app.route("/users", methods=["GET"])
def list_users():
    return []
"#,
    )?;
    fs::write(
        &java_file,
        r#"
package example;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class UserController {
    @GetMapping("/users")
    public String listUsers() {
        return "ok";
    }
}
"#,
    )?;
    fs::write(
        &csharp_file,
        r#"
using Xunit;

namespace Example;

public class UserBehavior
{
    [Fact]
    public void returns_users()
    {
    }

    [TestMethodAttribute]
    public void marker_suffix_case()
    {
    }
}
"#,
    )?;
    fs::write(
        &cpp_file,
        r#"
[[Test]]
void health_probe() {
}
"#,
    )?;

    let (handler, workspace_root, route) = test_handler_and_route(&temp_dir).await?;
    let result = run_indexing_pipeline(
        &workspace_tool(),
        &handler,
        vec![
            workspace_root.join("src/app.py"),
            workspace_root.join("src/main/java/example/UserController.java"),
            workspace_root.join("src/UserBehavior.cs"),
            workspace_root.join("src/native_marker.cpp"),
        ],
        &route,
        IndexingOperation::Incremental,
        &acquire_gate(&route.workspace_id).await,
    )
    .await?;

    assert_eq!(result.files_processed, 4, "four files should be indexed");
    assert_eq!(result.state.parsed_file_count(), 4);
    assert_eq!(result.state.repair_file_count(), 0);
    assert!(
        handler.indexing_status.search_ready.load(Ordering::Acquire),
        "mixed-language annotation indexing should publish search readiness"
    );

    let rows = annotation_rows(&handler, &route).await?;
    assert!(
        has_annotation(&rows, "list_users", "app.route"),
        "missing app.route row: {rows:?}"
    );
    assert!(
        has_annotation(&rows, "listUsers", "getmapping"),
        "missing getmapping row: {rows:?}"
    );
    assert!(
        has_annotation(&rows, "returns_users", "fact"),
        "missing fact row: {rows:?}"
    );
    assert!(
        has_annotation(&rows, "marker_suffix_case", "testmethod"),
        "missing suffix-normalized testmethod row: {rows:?}"
    );
    assert!(
        has_annotation(&rows, "health_probe", "test"),
        "missing annotation marker row: {rows:?}"
    );
    assert_eq!(
        parent_name_for_symbol(&handler, &route, "listUsers").await?,
        Some("UserController".to_string()),
        "Java method should retain its owner context"
    );

    let route_text = fast_search_text(&handler, "@app.route", None).await?;
    assert!(
        route_text.contains("list_users"),
        "@app.route should find Python handler, got: {route_text}"
    );

    let get_mapping_text = fast_search_text(&handler, "@GetMapping", None).await?;
    assert!(
        get_mapping_text.contains("listUsers"),
        "@GetMapping should find Java method, got: {get_mapping_text}"
    );

    let java_text = fast_search_text(&handler, "@GetMapping UserController", None).await?;
    assert!(
        java_text.contains("listUsers"),
        "@GetMapping UserController should find Java method, got: {java_text}"
    );

    let fact_text = fast_search_text(&handler, "@Fact", Some(false)).await?;
    assert!(
        fact_text.contains("returns_users"),
        "@Fact should find C# test method, got: {fact_text}"
    );

    let unprefixed_test_text = fast_search_text(&handler, "testmethod", Some(false)).await?;
    assert!(
        !unprefixed_test_text.contains("marker_suffix_case"),
        "unprefixed testmethod should not match annotation-key-only fields: {unprefixed_test_text}"
    );

    let excluded_fact_text = fast_search_text(&handler, "@Fact", Some(true)).await?;
    assert!(
        !excluded_fact_text.contains("returns_users"),
        "exclude_tests=true should filter annotation-detected C# test method: {excluded_fact_text}"
    );

    Ok(())
}



