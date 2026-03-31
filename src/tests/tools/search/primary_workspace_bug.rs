// Test for bug: fast_search fails with "Workspace not indexed" despite symbols being present
//
// BUG REPRODUCTION:
// - Primary workspace has 7,917 symbols indexed
// - fast_search(workspace="primary") returns "Workspace not indexed yet!"
// - Reference workspace search works fine
//
// This test verifies that fast_search correctly recognizes an indexed primary workspace

use crate::handler::JulieServerHandler;
use crate::tools::search::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_recognizes_indexed_primary_workspace() -> Result<()> {
    // Setup: Create a temporary workspace with actual symbols
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create a simple test file
    let test_file = workspace_path.join("test.rs");
    std::fs::write(
        &test_file,
        r#"
        pub struct TestStruct {
            pub field: String,
        }

        pub fn test_function() {
            println!("Hello");
        }
        "#,
    )?;

    // Initialize the Julie server handler
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Index the workspace using ManageWorkspaceTool
    // 🔥 CRITICAL: Must pass workspace_path explicitly - path: None uses current_dir()!
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(workspace_path.to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };

    index_tool.call_tool(&handler).await?;

    // Verify workspace is actually indexed
    if let Some(workspace) = handler.get_workspace().await? {
        if let Some(db) = &workspace.db {
            let db_lock = db.lock().unwrap();
            let symbol_count = db_lock.get_symbol_count_for_workspace()?;
            assert!(
                symbol_count > 0,
                "Test setup failed: workspace should have symbols indexed, got {}",
                symbol_count
            );
        }
    }

    // THE BUG: This should work but may return "Workspace not indexed yet!"
    let search_tool = FastSearchTool {
        query: "TestStruct".to_string(),
        limit: 10,
        search_target: "definitions".to_string(),
        file_pattern: None,
        language: None,
        context_lines: None,
        exclude_tests: None,
        workspace: Some("primary".to_string()), // Using "primary" should work!
        ..Default::default()
    };

    let result = search_tool.call_tool(&handler).await?;

    // Convert result to string for easier assertion
    let result_str = format!("{:?}", result);

    // ASSERTION: Should NOT get "Workspace not indexed yet!" error
    // This test will FAIL until the bug is fixed
    assert!(
        !result_str.contains("Workspace not indexed yet!"),
        "Bug reproduced: fast_search incorrectly reports workspace as not indexed.\n\
         Workspace has symbols but search claims it's not ready.\n\
         Response: {}",
        result_str
    );

    // ASSERTION: Should get actual search results or "No results" (not an error)
    assert!(
        result_str.contains("TestStruct")
            || result_str.contains("No results")
            || result_str.contains("🔍"),
        "Expected either search results or 'No results', got: {}",
        result_str
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_with_explicit_workspace_id() -> Result<()> {
    // This test verifies the bug also occurs with explicit workspace ID
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create a simple test file
    let test_file = workspace_path.join("test.rs");
    std::fs::write(
        &test_file,
        r#"
        pub fn another_function() {
            println!("Test");
        }
        "#,
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Index the workspace
    // 🔥 CRITICAL: Must pass workspace_path explicitly - path: None uses current_dir()!
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(workspace_path.to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };

    index_tool.call_tool(&handler).await?;

    // Get the actual workspace ID — compute directly from path (index.rs no longer
    // writes registry.json in stdio mode; it uses generate_workspace_id for embeddings)
    let workspace_id = if let Some(workspace) = handler.get_workspace().await? {
        crate::workspace::registry::generate_workspace_id(&workspace.root.to_string_lossy())
            .expect("Should be able to generate workspace ID from path")
    } else {
        panic!("No workspace found");
    };

    // Search using the explicit workspace ID
    let search_tool = FastSearchTool {
        query: "another_function".to_string(),
        limit: 10,
        search_target: "definitions".to_string(),
        file_pattern: None,
        language: None,
        context_lines: None,
        exclude_tests: None,
        workspace: Some(workspace_id), // Using actual workspace ID
        ..Default::default()
    };

    let result = search_tool.call_tool(&handler).await?;
    let result_str = format!("{:?}", result);

    // Should NOT get "Workspace not indexed yet!" error
    assert!(
        !result_str.contains("Workspace not indexed yet!"),
        "Bug reproduced with explicit workspace ID: {}",
        result_str
    );

    Ok(())
}
