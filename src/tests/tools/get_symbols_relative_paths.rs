//! Tests for get_symbols tool with relative Unix-style path storage
//!
//! These tests verify that get_symbols correctly handles relative paths
//! after Phase 2 implementation (relative Unix-style path storage).

use crate::handler::JulieServerHandler;
use crate::tools::symbols::GetSymbolsTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

/// Test that get_symbols can find symbols when given a relative path
///
/// After Phase 2, database stores relative Unix-style paths like "src/main.rs"
/// The tool should accept relative paths and find symbols correctly.
#[tokio::test]
async fn test_get_symbols_with_relative_path() -> Result<()> {
    // Setup: Create temp workspace with a Rust file
    let temp_dir = TempDir::new()?;
    let src_dir = temp_dir.path().join("src");
    fs::create_dir(&src_dir)?;

    let test_file = src_dir.join("main.rs");
    fs::write(&test_file, r#"
        pub fn get_user_data(id: u32) -> String {
            format!("User {}", id)
        }

        pub struct UserService {
            pub name: String,
        }
    "#)?;

    // Initialize workspace and index
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // TEST: Call get_symbols with RELATIVE path (Phase 2 format)
    let tool = GetSymbolsTool {
        file_path: "src/main.rs".to_string(), // RELATIVE, not absolute!
        max_depth: 1,
        mode: None,
        limit: None,
        target: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    // ASSERTION: Should find symbols (currently fails)
    assert!(
        result_text.contains("get_user_data") || result_text.contains("UserService"),
        "Should find symbols with relative path input, got: {}",
        result_text
    );

    // Verify no "No symbols found" message
    assert!(
        !result_text.contains("No symbols found"),
        "Should not return 'No symbols found' for valid relative path"
    );

    Ok(())
}

/// Test that get_symbols can find symbols when given an absolute path
///
/// Even with relative storage, the tool should accept absolute paths
/// and convert them to relative before querying.
#[tokio::test]
async fn test_get_symbols_with_absolute_path() -> Result<()> {
    // Setup: Create temp workspace with a Rust file
    let temp_dir = TempDir::new()?;
    let src_dir = temp_dir.path().join("src");
    fs::create_dir(&src_dir)?;

    let test_file = src_dir.join("lib.rs");
    fs::write(&test_file, r#"
        pub fn calculate_score(points: i32) -> i32 {
            points * 2
        }
    "#)?;

    // Initialize workspace and index
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // TEST: Call get_symbols with ABSOLUTE path
    let absolute_path = test_file.to_string_lossy().to_string();
    let tool = GetSymbolsTool {
        file_path: absolute_path.clone(),
        max_depth: 1,
        mode: None,
        limit: None,
        target: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    // ASSERTION: Should find symbols (currently fails)
    assert!(
        result_text.contains("calculate_score"),
        "Should find symbols with absolute path input (converted to relative), got: {}",
        result_text
    );

    Ok(())
}

/// Test that database actually stores relative Unix-style paths
///
/// This is the foundation test - verify Phase 2 storage is working.
#[tokio::test]
async fn test_database_stores_relative_unix_paths() -> Result<()> {
    use crate::database::SymbolDatabase;

    // Setup: Create temp workspace
    let temp_dir = TempDir::new()?;
    let src_dir = temp_dir.path().join("src");
    let tools_dir = src_dir.join("tools");
    fs::create_dir_all(&tools_dir)?;

    let test_file = tools_dir.join("search.rs");
    fs::write(&test_file, r#"
        pub fn search_code() {}
    "#)?;

    // Initialize and index
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // Get database and query directly
    let workspace = handler.get_workspace().await?.expect("Workspace should exist");
    let db = workspace.db.as_ref().expect("Database should exist");
    let db_lock = db.lock().unwrap();

    // Query all symbols
    let all_symbols = db_lock.get_all_symbols()?;

    // ASSERTION: Paths should be relative Unix-style
    let paths: Vec<String> = all_symbols.iter().map(|s| s.file_path.clone()).collect();

    // Should have at least one symbol from our test file
    assert!(!paths.is_empty(), "Should have indexed symbols");

    // Check that paths are relative (don't start with /)
    for path in &paths {
        assert!(
            !path.starts_with('/'),
            "Path should be relative, not absolute: {}",
            path
        );

        assert!(
            !path.contains('\\'),
            "Path should use Unix-style separators, not backslashes: {}",
            path
        );
    }

    // Should find our specific file with relative path
    let search_file_symbols: Vec<_> = all_symbols.iter()
        .filter(|s| s.file_path == "src/tools/search.rs")
        .collect();

    assert!(
        !search_file_symbols.is_empty(),
        "Should find symbols with relative path 'src/tools/search.rs'"
    );

    Ok(())
}
