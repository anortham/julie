//! Test for GetSymbolsTool bug with reference workspaces
//!
//! BUG: GetSymbolsTool is missing workspace parameter, always queries primary workspace
//! SYMPTOM: Returns "No symbols found" for reference workspace files even though symbols exist
//! ROOT CAUSE: GetSymbolsTool struct doesn't have workspace: Option<String> field

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::SymbolKind;
use crate::database::SymbolDatabase;
use crate::extractors::base::Symbol;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::{GetSymbolsTool, ManageWorkspaceTool};
use crate::workspace::registry::{
    RegistryConfig, WorkspaceEntry, WorkspaceRegistry, WorkspaceType,
};

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
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
        .join("\n")
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
        "pub struct PrimaryStruct { pub field: String }\n",
    )?;

    // Create reference workspace
    let reference_dir = TempDir::new()?;
    let reference_path = reference_dir.path().to_path_buf();
    let reference_src = reference_path.join("src");
    fs::create_dir_all(&reference_src)?;

    let reference_file_path = reference_src.join("reference.rs");
    fs::write(
        &reference_file_path,
        "pub struct ReferenceStruct { pub data: i32 }\npub fn reference_function() {}\n",
    )?;

    // Initialize handler with primary workspace and index it
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(primary_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // In stdio mode, ManageWorkspaceTool::add requires daemon mode, so we manually
    // create the reference workspace database at the expected path.
    // (This mirrors what add+index does in daemon mode.)
    let workspace = handler.get_workspace().await?.unwrap();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())?;

    let ref_db_path = workspace.workspace_db_path(&workspace_id);
    fs::create_dir_all(ref_db_path.parent().unwrap())?;

    {
        let mut ref_db = SymbolDatabase::new(&ref_db_path)?;
        let ref_file = reference_file_path.to_string_lossy().to_string();
        ref_db.bulk_store_symbols(
            &[
                Symbol {
                    id: "ref_struct_1".to_string(),
                    name: "ReferenceStruct".to_string(),
                    kind: SymbolKind::Struct,
                    language: "rust".to_string(),
                    file_path: ref_file.clone(),
                    signature: Some("pub struct ReferenceStruct".to_string()),
                    start_line: 1,
                    start_column: 0,
                    end_line: 1,
                    end_column: 44,
                    start_byte: 0,
                    end_byte: 44,
                    doc_comment: None,
                    visibility: None,
                    parent_id: None,
                    metadata: None,
                    semantic_group: None,
                    confidence: None,
                    code_context: None,
                    content_type: None,
                },
                Symbol {
                    id: "ref_fn_1".to_string(),
                    name: "reference_function".to_string(),
                    kind: SymbolKind::Function,
                    language: "rust".to_string(),
                    file_path: ref_file.clone(),
                    signature: Some("pub fn reference_function()".to_string()),
                    start_line: 2,
                    start_column: 0,
                    end_line: 2,
                    end_column: 30,
                    start_byte: 45,
                    end_byte: 75,
                    doc_comment: None,
                    visibility: None,
                    parent_id: None,
                    metadata: None,
                    semantic_group: None,
                    confidence: None,
                    code_context: None,
                    content_type: None,
                },
            ],
            &workspace_id,
        )?;
    }

    let reference_file_str = reference_file_path.to_string_lossy().to_string();

    let get_symbols_tool = GetSymbolsTool {
        file_path: reference_file_str.clone(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: None,
        workspace: Some(workspace_id.clone()),
    };

    let result = get_symbols_tool.call_tool(&handler).await?;
    let result_text = extract_text_from_result(&result);

    assert!(
        !result_text.contains("No symbols found"),
        "BUG REPRODUCED: get_symbols returned 'No symbols found' for reference workspace file.\n\
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

/// Test that filtering parameters (max_depth, target, limit) work in reference workspace
///
/// BUG: The get_symbols_from_reference method ignores ALL filtering logic
/// that exists in the primary workspace code path
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
        "pub struct PrimaryStruct { pub field: String }\n",
    )?;

    // Create reference workspace with nested symbols
    let reference_dir = TempDir::new()?;
    let reference_path = reference_dir.path().to_path_buf();
    let reference_src = reference_path.join("src");
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

    // Initialize handler with primary workspace and index it
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(primary_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // Manually create the reference workspace database (daemon required for add in stdio mode)
    let workspace = handler.get_workspace().await?.unwrap();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())?;

    let ref_db_path = workspace.workspace_db_path(&workspace_id);
    fs::create_dir_all(ref_db_path.parent().unwrap())?;

    let nested_file = nested_file_path.to_string_lossy().to_string();
    let outer_id = "ref_outer_struct";

    {
        let mut ref_db = SymbolDatabase::new(&ref_db_path)?;
        ref_db.bulk_store_symbols(
            &[
                Symbol {
                    id: outer_id.to_string(),
                    name: "Outer".to_string(),
                    kind: SymbolKind::Struct,
                    language: "rust".to_string(),
                    file_path: nested_file.clone(),
                    signature: Some("pub struct Outer".to_string()),
                    start_line: 1,
                    start_column: 0,
                    end_line: 1,
                    end_column: 33,
                    start_byte: 0,
                    end_byte: 33,
                    doc_comment: None,
                    visibility: None,
                    parent_id: None,
                    metadata: None,
                    semantic_group: None,
                    confidence: None,
                    code_context: None,
                    content_type: None,
                },
                Symbol {
                    id: "ref_method_one".to_string(),
                    name: "method_one".to_string(),
                    kind: SymbolKind::Method,
                    language: "rust".to_string(),
                    file_path: nested_file.clone(),
                    signature: Some("pub fn method_one(&self)".to_string()),
                    start_line: 3,
                    start_column: 4,
                    end_line: 3,
                    end_column: 30,
                    start_byte: 50,
                    end_byte: 80,
                    doc_comment: None,
                    visibility: None,
                    parent_id: Some(outer_id.to_string()),
                    metadata: None,
                    semantic_group: None,
                    confidence: None,
                    code_context: None,
                    content_type: None,
                },
                Symbol {
                    id: "ref_method_two".to_string(),
                    name: "method_two".to_string(),
                    kind: SymbolKind::Method,
                    language: "rust".to_string(),
                    file_path: nested_file.clone(),
                    signature: Some("pub fn method_two(&self)".to_string()),
                    start_line: 4,
                    start_column: 4,
                    end_line: 4,
                    end_column: 30,
                    start_byte: 81,
                    end_byte: 111,
                    doc_comment: None,
                    visibility: None,
                    parent_id: Some(outer_id.to_string()),
                    metadata: None,
                    semantic_group: None,
                    confidence: None,
                    code_context: None,
                    content_type: None,
                },
                Symbol {
                    id: "ref_outer_fn".to_string(),
                    name: "outer_function".to_string(),
                    kind: SymbolKind::Function,
                    language: "rust".to_string(),
                    file_path: nested_file.clone(),
                    signature: Some("pub fn outer_function()".to_string()),
                    start_line: 6,
                    start_column: 0,
                    end_line: 6,
                    end_column: 27,
                    start_byte: 120,
                    end_byte: 147,
                    doc_comment: None,
                    visibility: None,
                    parent_id: None,
                    metadata: None,
                    semantic_group: None,
                    confidence: None,
                    code_context: None,
                    content_type: None,
                },
                Symbol {
                    id: "ref_another_struct".to_string(),
                    name: "Another".to_string(),
                    kind: SymbolKind::Struct,
                    language: "rust".to_string(),
                    file_path: nested_file.clone(),
                    signature: Some("pub struct Another".to_string()),
                    start_line: 7,
                    start_column: 0,
                    end_line: 7,
                    end_column: 38,
                    start_byte: 148,
                    end_byte: 186,
                    doc_comment: None,
                    visibility: None,
                    parent_id: None,
                    metadata: None,
                    semantic_group: None,
                    confidence: None,
                    code_context: None,
                    content_type: None,
                },
            ],
            &workspace_id,
        )?;
    }

    let nested_file_str = nested_file_path.to_string_lossy().to_string();

    // TEST 1: Get all symbols without filtering
    let get_all = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 999,
        target: None,
        limit: None,
        mode: None,
        workspace: Some(workspace_id.clone()),
    };

    let result_all = get_all.call_tool(&handler).await?;
    let text_all = extract_text_from_result(&result_all);

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
    };

    let result_depth_0 = get_depth_0.call_tool(&handler).await?;
    let text_depth_0 = extract_text_from_result(&result_depth_0);

    assert!(
        !text_depth_0.is_empty(),
        "max_depth=0 should return some symbols"
    );

    // TEST 3: target="Outer" should return Outer and its children (methods)
    let get_target = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 999,
        target: Some("Outer".to_string()),
        limit: None,
        mode: None,
        workspace: Some(workspace_id.clone()),
    };

    let result_target = get_target.call_tool(&handler).await?;
    let text_target = extract_text_from_result(&result_target);

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

    // TEST 4: limit=2 should only return 2 top-level symbols
    let get_limit = GetSymbolsTool {
        file_path: nested_file_str.clone(),
        max_depth: 999,
        target: None,
        limit: Some(2),
        mode: None,
        workspace: Some(workspace_id.clone()),
    };

    let result_limit = get_limit.call_tool(&handler).await?;
    let text_limit = extract_text_from_result(&result_limit);

    assert!(!text_limit.is_empty(), "limit=2 should return some symbols");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_get_symbols_reference_workspace_relative_paths_after_primary_rebind() -> Result<()> {
    let first_primary_dir = TempDir::new()?;
    let rebound_primary_dir = TempDir::new()?;
    let reference_dir = TempDir::new()?;

    let first_primary_path = first_primary_dir.path().to_path_buf();
    let rebound_primary_path = rebound_primary_dir.path().to_path_buf();
    let reference_path = reference_dir.path().to_path_buf();

    fs::create_dir_all(first_primary_path.join("src"))?;
    fs::create_dir_all(rebound_primary_path.join("src"))?;
    fs::create_dir_all(reference_path.join("src"))?;

    fs::write(
        first_primary_path.join("src").join("old.rs"),
        "fn old_primary() {}\n",
    )?;
    fs::write(
        rebound_primary_path.join("src").join("new.rs"),
        "fn rebound_primary() {}\n",
    )?;
    let reference_file_path = reference_path.join("src").join("reference.rs");
    fs::write(
        &reference_file_path,
        "pub fn rebound_reference_symbol() {\n    println!(\"reference body\");\n}\n",
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(
            Some(first_primary_path.to_string_lossy().to_string()),
            true,
        )
        .await?;
    handler
        .initialize_workspace_with_force(
            Some(rebound_primary_path.to_string_lossy().to_string()),
            true,
        )
        .await?;

    let reference_workspace_id =
        crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())?;
    let ref_db_path = rebound_primary_path
        .join(".julie")
        .join("indexes")
        .join(&reference_workspace_id)
        .join("db")
        .join("symbols.db");
    fs::create_dir_all(ref_db_path.parent().unwrap())?;

    {
        let mut ref_db = SymbolDatabase::new(&ref_db_path)?;
        ref_db.bulk_store_fresh_atomic(
            &[crate::database::types::FileInfo {
                path: "src/reference.rs".to_string(),
                language: "rust".to_string(),
                hash: "ref-hash".to_string(),
                size: 1,
                last_modified: 1,
                last_indexed: 1,
                symbol_count: 1,
                line_count: 3,
                content: Some(
                    "pub fn rebound_reference_symbol() {\n    println!(\"reference body\");\n}\n"
                        .to_string(),
                ),
            }],
            &[Symbol {
                id: "ref_fn_1".to_string(),
                name: "rebound_reference_symbol".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/reference.rs".to_string(),
                signature: Some("pub fn rebound_reference_symbol()".to_string()),
                start_line: 1,
                start_column: 0,
                end_line: 3,
                end_column: 1,
                start_byte: 0,
                end_byte: 68,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
            }],
            &[],
            &[],
            &[],
            &reference_workspace_id,
        )?;
    }

    let config = RegistryConfig::default();
    let reference_entry = WorkspaceEntry::new(
        reference_path.to_string_lossy().to_string(),
        WorkspaceType::Reference,
        &config,
    )?;
    let mut registry = WorkspaceRegistry::default();
    registry
        .reference_workspaces
        .insert(reference_workspace_id.clone(), reference_entry);
    let registry_path = rebound_primary_path
        .join(".julie")
        .join("workspace_registry.json");
    fs::create_dir_all(registry_path.parent().unwrap())?;
    fs::write(&registry_path, serde_json::to_string_pretty(&registry)?)?;

    let get_symbols_tool = GetSymbolsTool {
        file_path: "src/reference.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        mode: Some("full".to_string()),
        workspace: Some(reference_workspace_id.clone()),
    };

    let result = get_symbols_tool.call_tool(&handler).await?;
    let result_text = extract_text_from_result(&result);

    assert!(
        result_text.contains("rebound_reference_symbol"),
        "reference get_symbols should resolve relative paths under the reference root after primary rebind: {}",
        result_text
    );
    assert!(
        result_text.contains("reference body"),
        "body extraction should read from the reference root, not the stale loaded root: {}",
        result_text
    );

    for file_path in ["./src/reference.rs", "src/../src/reference.rs"] {
        let get_symbols_tool = GetSymbolsTool {
            file_path: file_path.to_string(),
            max_depth: 1,
            target: None,
            limit: None,
            mode: Some("full".to_string()),
            workspace: Some(reference_workspace_id.clone()),
        };

        let result = get_symbols_tool.call_tool(&handler).await?;
        let result_text = extract_text_from_result(&result);
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

#[tokio::test(flavor = "multi_thread")]
async fn test_get_workspace_root_for_target_rejects_swap_gap_root_resolution() -> Result<()> {
    let primary_dir = TempDir::new()?;
    let primary_path = primary_dir.path().to_path_buf();
    fs::create_dir_all(primary_path.join("src"))?;
    fs::write(
        primary_path.join("src").join("primary.rs"),
        "fn primary() {}\n",
    )?;

    let reference_dir = TempDir::new()?;
    let reference_path = reference_dir.path().to_path_buf();
    fs::create_dir_all(reference_path.join("src"))?;
    fs::write(
        reference_path.join("src").join("reference.rs"),
        "pub fn reference_symbol() {}\n",
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
        .await?;

    let reference_workspace_id =
        crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())?;
    let ref_db_path = primary_path
        .join(".julie")
        .join("indexes")
        .join(&reference_workspace_id)
        .join("db")
        .join("symbols.db");
    fs::create_dir_all(ref_db_path.parent().unwrap())?;

    {
        let mut ref_db = SymbolDatabase::new(&ref_db_path)?;
        ref_db.bulk_store_fresh_atomic(
            &[crate::database::types::FileInfo {
                path: "src/reference.rs".to_string(),
                language: "rust".to_string(),
                hash: "ref-hash".to_string(),
                size: 1,
                last_modified: 1,
                last_indexed: 1,
                symbol_count: 1,
                line_count: 1,
                content: Some("pub fn reference_symbol() {}\n".to_string()),
            }],
            &[Symbol {
                id: "ref_fn_1".to_string(),
                name: "reference_symbol".to_string(),
                kind: SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/reference.rs".to_string(),
                signature: Some("pub fn reference_symbol()".to_string()),
                start_line: 1,
                start_column: 0,
                end_line: 1,
                end_column: 27,
                start_byte: 0,
                end_byte: 27,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
            }],
            &[],
            &[],
            &[],
            &reference_workspace_id,
        )?;
    }

    let config = RegistryConfig::default();
    let reference_entry = WorkspaceEntry::new(
        reference_path.to_string_lossy().to_string(),
        WorkspaceType::Reference,
        &config,
    )?;
    let mut registry = WorkspaceRegistry::default();
    registry
        .reference_workspaces
        .insert(reference_workspace_id.clone(), reference_entry);
    let registry_path = primary_path.join(".julie").join("workspace_registry.json");
    fs::create_dir_all(registry_path.parent().unwrap())?;
    fs::write(&registry_path, serde_json::to_string_pretty(&registry)?)?;

    handler.publish_loaded_workspace_swap_intent_for_test();

    let err = handler
        .get_workspace_root_for_target(&reference_workspace_id)
        .await
        .expect_err("swap gap should reject reference-target root resolution");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );

    Ok(())
}
