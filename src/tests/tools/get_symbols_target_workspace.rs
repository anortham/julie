//! Tests for the GetSymbolsTool target-workspace path.

use anyhow::Result;
use julie_context::WorkspaceTarget;
use julie_test_support::{FakeToolContext, SnapshotFixture};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tools::GetSymbolsTool;
use crate::workspace::registry::generate_workspace_id;

fn target_context(reference_root: &Path) -> Result<(FakeToolContext, String)> {
    let workspace_id = generate_workspace_id(&reference_root.to_string_lossy())?;
    let fixture = SnapshotFixture::from_tree(reference_root)?;
    let context = FakeToolContext::new()
        .with_workspace_id("primary-workspace")
        .with_primary_root(fixture.root().to_path_buf())
        .with_resolved_target(WorkspaceTarget::Target(workspace_id.clone()))
        .with_snapshot_fixture(fixture);
    Ok((context, workspace_id))
}

#[tokio::test(flavor = "multi_thread")]
async fn test_get_symbols_target_workspace() -> Result<()> {
    let reference_dir = TempDir::new()?;
    let reference_src = reference_dir.path().join("src");
    fs::create_dir_all(&reference_src)?;
    let reference_file_path = reference_src.join("reference.rs");
    fs::write(
        &reference_file_path,
        "pub struct ReferenceStruct { pub data: i32 }\npub fn reference_function() {}\n",
    )?;
    let (handler, workspace_id) = target_context(reference_dir.path())?;
    let reference_file_str = reference_file_path.to_string_lossy().to_string();

    let get_symbols_tool = GetSymbolsTool {
        file_path: reference_file_str.clone(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: Some(workspace_id.clone()),
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };
    let result = get_symbols_tool.call_tool(&handler).await?;
    let result_text = call_tool_result_text(&result);

    assert!(
        !result_text.contains("No symbols found"),
        "BUG REPRODUCED: get_symbols returned 'No symbols found' for target-workspace file.\n\
         File: {}\n\
         Workspace: {}\n\
         Response: {}",
        reference_file_str,
        workspace_id,
        result_text
    );
    assert!(
        result_text.contains("ReferenceStruct") || result_text.contains("reference_function"),
        "Should find ReferenceStruct or reference_function, got: {}",
        result_text
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_get_symbols_target_workspace_filtering() -> Result<()> {
    let reference_dir = TempDir::new()?;
    let reference_src = reference_dir.path().join("src");
    fs::create_dir_all(&reference_src)?;
    let nested_file_path = reference_src.join("nested.rs");
    fs::write(
        &nested_file_path,
        r#"pub struct Outer { pub data: i32 }
impl Outer {
    pub fn method_one(&self) {}
    pub fn method_two(&self) {}
}
pub fn outer_function() {}
pub struct Another { pub field: String }
"#,
    )?;
    let (handler, workspace_id) = target_context(reference_dir.path())?;
    let nested_file_str = nested_file_path.to_string_lossy().to_string();

    let get_all = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 999,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: Some(workspace_id.clone()),
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };
    let text_all = call_tool_result_text(&get_all.call_tool(&handler).await?);

    assert!(
        text_all.contains("Outer"),
        "Should find Outer struct in target workspace: {}",
        text_all
    );
    assert!(
        text_all.contains("outer_function"),
        "Should find outer_function in target workspace: {}",
        text_all
    );
    assert!(
        text_all.contains("Another"),
        "Should find Another struct in target workspace: {}",
        text_all
    );

    let get_depth_0 = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 0,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: Some(workspace_id.clone()),
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };
    let text_depth_0 = call_tool_result_text(&get_depth_0.call_tool(&handler).await?);

    assert!(
        !text_depth_0.is_empty(),
        "max_depth=0 should return some symbols"
    );

    let get_target = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 999,
        target: Some("Outer".to_string()),
        limit: None,
        offset: 0,
        mode: None,
        workspace: Some(workspace_id.clone()),
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };
    let text_target = call_tool_result_text(&get_target.call_tool(&handler).await?);

    assert!(
        text_target.contains("Outer"),
        "target filtering should find 'Outer' symbol: {}",
        text_target
    );
    assert!(
        !text_target.contains("Another"),
        "target filtering should exclude 'Another' that doesn't match target: {}",
        text_target
    );

    let get_limit = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 999,
        target: None,
        limit: Some(2),
        offset: 0,
        mode: None,
        workspace: Some(workspace_id.clone()),
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };
    let text_limit = call_tool_result_text(&get_limit.call_tool(&handler).await?);

    assert!(!text_limit.is_empty(), "limit=2 should return some symbols");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_get_symbols_target_workspace_relative_paths_after_primary_rebind() -> Result<()> {
    let reference_dir = TempDir::new()?;
    fs::create_dir_all(reference_dir.path().join("src"))?;
    fs::write(
        reference_dir.path().join("src").join("reference.rs"),
        "pub fn rebound_reference_symbol() {\n    println!(\"reference body\");\n}\n",
    )?;
    let (handler, target_workspace_id) = target_context(reference_dir.path())?;

    for file_path in [
        "src/reference.rs",
        "./src/reference.rs",
        "src/../src/reference.rs",
    ] {
        let get_symbols_tool = GetSymbolsTool {
            file_path: file_path.to_string(),
            max_depth: 1,
            target: None,
            limit: None,
            offset: 0,
            mode: Some("full".to_string()),
            workspace: Some(target_workspace_id.clone()),
            body_offset: 0,
            body_limit: None,
            source_hash: None,
        };
        let result = get_symbols_tool.call_tool(&handler).await?;
        let result_text = call_tool_result_text(&result);

        assert!(
            result_text.contains("rebound_reference_symbol"),
            "reference get_symbols should normalize relative path variant '{}' against the reference root: {}",
            file_path,
            result_text
        );
        assert!(
            result_text.contains("reference body"),
            "body extraction should succeed for relative path variant '{}': {}",
            file_path,
            result_text
        );
    }
    Ok(())
}
