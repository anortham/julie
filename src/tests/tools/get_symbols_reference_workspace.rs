//! Test for GetSymbolsTool bug with reference workspaces
//!
//! BUG: GetSymbolsTool is missing workspace parameter, always queries primary workspace
//! SYMPTOM: Returns "No symbols found" for reference workspace files even though symbols exist
//! ROOT CAUSE: GetSymbolsTool struct doesn't have workspace: Option<String> field

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::{GetSymbolsTool, ManageWorkspaceTool};
use crate::mcp_compat::{CallToolResult, CallToolResultExt, StructuredContentExt};

/// Extract text from CallToolResult safely (handles both TOON and JSON modes)
fn extract_text_from_result(result: &CallToolResult) -> String {
    // Try extracting from .content first (TOON mode)
    if !result.content.is_empty() {
        return result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
            })
            .collect::<Vec<_>>()
            .join("
");
    }

    // Fall back to .structured_content (JSON mode)
    if let Some(structured) = result.structured_content() {
        return serde_json::to_string_pretty(&structured).unwrap_or_default();
    }

    String::new()
}

#[tokio::test(flavor = "multi_thread")]
async fn test_get_symbols_reference_workspace() -> Result<()> {
    // BUG REPRODUCTION:
    // - Primary workspace indexed with symbols
    // - Reference workspace added and indexed with different symbols
    // - get_symbols(file_from_reference_workspace) returns "No symbols found"
    // - fast_search finds those same symbols just fine
    //
    // ROOT CAUSE: GetSymbolsTool missing workspace parameter, always queries primary DB

    // Create primary workspace
    let primary_dir = TempDir::new()?;
    let primary_path = primary_dir.path().to_path_buf();
    let primary_src = primary_path.join("src");
    fs::create_dir_all(&primary_src)?;

    fs::write(
        primary_src.join("primary.rs"),
        r#"
pub struct PrimaryStruct {
    pub field: String,
}
"#,
    )?;

    // Create reference workspace
    let reference_dir = TempDir::new()?;
    let reference_path = reference_dir.path().to_path_buf();
    let reference_src = reference_path.join("src");
    fs::create_dir_all(&reference_src)?;

    fs::write(
        reference_src.join("reference.rs"),
        r#"
pub struct ReferenceStruct {
    pub data: i32,
}

pub fn reference_function() {
    println!("Reference");
}
"#,
    )?;

    // Initialize handler with primary workspace
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
        .await?;

    // Index primary workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(primary_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // Add reference workspace
    let add_tool = ManageWorkspaceTool {
        operation: "add".to_string(),
        path: Some(reference_path.to_string_lossy().to_string()),
        name: Some("test-reference".to_string()),
        force: None,
        workspace_id: None,
        detailed: None,
    };
    let add_result = add_tool.call_tool(&handler).await?;
    let add_text = extract_text_from_result(&add_result);

    // Extract workspace ID from add result
    // Format: "Workspace ID: reference_xxxxxxxx"
    let workspace_id = add_text
        .lines()
        .find(|line| line.starts_with("Workspace ID:"))
        .and_then(|line| line.split(':').nth(1))
        .map(|id| id.trim().to_string())
        .expect("Should get workspace ID from add result");

    // Construct full path to reference file
    let reference_file = reference_src.join("reference.rs");
    let reference_file_str = reference_file.to_string_lossy().to_string();

    // THE BUG: GetSymbolsTool doesn't have workspace parameter yet
    // When we add it, this test structure will need updating
    // For now, this demonstrates the bug - it will return "No symbols found"
    // even though the file exists and has symbols

    let get_symbols_tool = GetSymbolsTool {
        file_path: reference_file_str.clone(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: None,
        workspace: Some(workspace_id.clone()), // ✅ NOW IT EXISTS!
        output_format: None,
    };

    let result = get_symbols_tool.call_tool(&handler).await?;
    let result_text = extract_text_from_result(&result);

    // ASSERTION: This test will FAIL until we add workspace parameter
    // Currently returns "No symbols found" because it queries primary workspace DB
    // After fix, should return ReferenceStruct and reference_function symbols
    assert!(
        !result_text.contains("No symbols found"),
        "BUG REPRODUCED: get_symbols returned 'No symbols found' for reference workspace file.\n\
         File: {}\n\
         Workspace: {}\n\
         This happens because GetSymbolsTool is missing workspace parameter and always queries primary workspace.\n\
         Response: {}",
        reference_file_str,
        workspace_id,
        result_text
    );

    // After fix, verify we got the expected symbols
    assert!(
        result_text.contains("ReferenceStruct") || result_text.contains("reference_function"),
        "After fix: Should find ReferenceStruct or reference_function, got: {}",
        result_text
    );

    Ok(())
}

/// Test that filtering parameters (max_depth, target, limit) work in reference workspace
///
/// BUG: The get_symbols_from_reference method ignores ALL filtering logic
/// that exists in the primary workspace code path (lines 204-405 in symbols.rs)
///
/// This test creates a reference workspace with nested symbols and verifies that:
/// - max_depth parameter limits the symbol depth
/// - target parameter filters to matching symbols + their descendants
/// - limit parameter restricts top-level symbols while including all children
#[tokio::test(flavor = "multi_thread")]
async fn test_get_symbols_reference_workspace_filtering() -> Result<()> {
    // Create primary workspace
    let primary_dir = TempDir::new()?;
    let primary_path = primary_dir.path().to_path_buf();
    let primary_src = primary_path.join("src");
    fs::create_dir_all(&primary_src)?;

    fs::write(
        primary_src.join("primary.rs"),
        r#"
pub struct PrimaryStruct {
    pub field: String,
}
"#,
    )?;

    // Create reference workspace with nested symbols
    let reference_dir = TempDir::new()?;
    let reference_path = reference_dir.path().to_path_buf();
    let reference_src = reference_path.join("src");
    fs::create_dir_all(&reference_src)?;

    // File with nested symbols (impl blocks, methods, nested types)
    fs::write(
        reference_src.join("nested.rs"),
        r#"
pub struct Outer {
    pub data: i32,
}

impl Outer {
    pub fn method_one(&self) {
        println!("method_one");
    }

    pub fn method_two(&self) {
        println!("method_two");
    }
}

pub fn outer_function() {
    println!("outer");
}

pub struct Another {
    pub field: String,
}
"#,
    )?;

    // Initialize handler with primary workspace
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
        .await?;

    // Index primary workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(primary_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // Add reference workspace
    let add_tool = ManageWorkspaceTool {
        operation: "add".to_string(),
        path: Some(reference_path.to_string_lossy().to_string()),
        name: Some("test-reference-filtering".to_string()),
        force: None,
        workspace_id: None,
        detailed: None,
    };
    let add_result = add_tool.call_tool(&handler).await?;
    let add_text = extract_text_from_result(&add_result);

    // Extract workspace ID from add result
    let workspace_id = add_text
        .lines()
        .find(|line| line.starts_with("Workspace ID:"))
        .and_then(|line| line.split(':').nth(1))
        .map(|id| id.trim().to_string())
        .expect("Should get workspace ID from add result");

    let nested_file = reference_src.join("nested.rs");
    let nested_file_str = nested_file.to_string_lossy().to_string();

    // TEST 1: Get all symbols without filtering
    // This should return all top-level symbols (Outer, outer_function, Another)
    let get_all = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 999, // Very deep
        target: None,
        limit: None,
        mode: None,
        workspace: Some(workspace_id.clone()),
        output_format: None,
    };

    let result_all = get_all.call_tool(&handler).await?;
    let text_all = extract_text_from_result(&result_all);

    // Should find all top-level symbols
    assert!(
        text_all.contains("Outer"),
        "Should find Outer struct in reference workspace: {}",
        text_all
    );
    assert!(
        text_all.contains("outer_function"),
        "Should find outer_function in reference workspace: {}",
        text_all
    );
    assert!(
        text_all.contains("Another"),
        "Should find Another struct in reference workspace: {}",
        text_all
    );

    // TEST 2: max_depth=0 should only return top-level symbols (no methods)
    let get_depth_0 = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 0,
        target: None,
        limit: None,
        mode: None,
        workspace: Some(workspace_id.clone()),
        output_format: None,
    };

    let result_depth_0 = get_depth_0.call_tool(&handler).await?;
    let _text_depth_0 = extract_text_from_result(&result_depth_0);
    let json_depth_0 = result_depth_0.structured_content();

    if let Some(ref json) = json_depth_0 {
        if let Some(symbols) = json.get("symbols") {
            if let Some(arr) = symbols.as_array() {
                // With max_depth=0, should only get top-level symbols (no methods)
                assert!(
                    arr.iter().all(|s| {
                        s.get("parent_id").is_none() || s.get("parent_id").unwrap().is_null()
                    }),
                    "max_depth=0 should only return top-level symbols (no parent_id), got: {:?}",
                    arr
                );
            }
        }
    }

    // TEST 3: target="Outer" should return Outer and its children (methods)
    let get_target = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 999,
        target: Some("Outer".to_string()),
        limit: None,
        mode: None,
        workspace: Some(workspace_id.clone()),
        output_format: None,
    };

    let result_target = get_target.call_tool(&handler).await?;
    let text_target = extract_text_from_result(&result_target);

    // Should find Outer
    assert!(
        text_target.contains("Outer"),
        "target filtering should find 'Outer' symbol: {}",
        text_target
    );

    // Should NOT find Another (doesn't match target)
    assert!(
        !text_target.contains("Another"),
        "target filtering should exclude 'Another' that doesn't match target: {}",
        text_target
    );

    // TEST 4: limit=2 should only return 2 top-level symbols (but all their children)
    let get_limit = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 999,
        target: None,
        limit: Some(2),
        mode: None,
        workspace: Some(workspace_id.clone()),
        output_format: None,
    };

    let result_limit = get_limit.call_tool(&handler).await?;
    let json_limit = result_limit.structured_content();

    if let Some(ref json) = json_limit {
        if let Some(symbols) = json.get("symbols") {
            if let Some(arr) = symbols.as_array() {
                let top_level_count = arr
                    .iter()
                    .filter(|s| {
                        s.get("parent_id").is_none() || s.get("parent_id").unwrap().is_null()
                    })
                    .count();

                assert!(
                    top_level_count <= 2,
                    "limit=2 should return at most 2 top-level symbols, got: {}",
                    top_level_count
                );
            }
        }
    }

    Ok(())
}
