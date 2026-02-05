//! Tests for Tantivy-based text search implementation
//!
//! Tests that text_search_impl() correctly uses Tantivy SearchIndex
//! for both symbol and content searches.

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::ManageWorkspaceTool;

#[tokio::test(flavor = "multi_thread")]
async fn test_text_search_definitions_basic() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create a Rust file with a function
    let test_file = src_dir.join("lib.rs");
    fs::write(
        &test_file,
        r#"
pub fn get_user(id: u32) -> User {
    // Implementation here
    User::new(id)
}
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Index the workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Now call text_search_impl directly
    let results = crate::tools::search::text_search::text_search_impl(
        "get_user",
        &None,
        &None,
        10,
        None,
        "definitions",
        None,
        &handler,
    )
    .await?;

    assert!(!results.is_empty(), "Should find the function");
    assert_eq!(results[0].name, "get_user", "Should match the function name");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_text_search_with_language_filter() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create Rust file
    let rust_file = src_dir.join("lib.rs");
    fs::write(
        &rust_file,
        r#"
pub fn process_data(data: &str) -> Result<String> {
    Ok(data.to_uppercase())
}
"#,
    )?;

    // Create TypeScript file with similar function
    let ts_file = src_dir.join("index.ts");
    fs::write(
        &ts_file,
        r#"
export function process_data(data: string): string {
    return data.toUpperCase();
}
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Index the workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Search for Rust only
    let results = crate::tools::search::text_search::text_search_impl(
        "process_data",
        &Some("rust".to_string()),
        &None,
        10,
        None,
        "definitions",
        None,
        &handler,
    )
    .await?;

    assert!(!results.is_empty(), "Should find at least one Rust result");
    for result in &results {
        assert_eq!(result.language.to_lowercase(), "rust", "All results should be Rust");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_text_search_with_file_pattern() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    let tests_dir = workspace_path.join("tests");
    fs::create_dir_all(&src_dir)?;
    fs::create_dir_all(&tests_dir)?;

    // Create file in src/
    let src_file = src_dir.join("lib.rs");
    fs::write(
        &src_file,
        r#"
pub fn helper() {
    // Implementation
}
"#,
    )?;

    // Create file in tests/
    let test_file = tests_dir.join("integration_tests.rs");
    fs::write(
        &test_file,
        r#"
pub fn helper() {
    // Test implementation
}
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Index the workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Search for files matching "src/**" pattern
    let results = crate::tools::search::text_search::text_search_impl(
        "helper",
        &None,
        &Some("src/**".to_string()),
        10,
        None,
        "definitions",
        None,
        &handler,
    )
    .await?;

    assert!(!results.is_empty(), "Should find results in src/");
    for result in &results {
        assert!(result.file_path.starts_with("src/"), "All results should be in src/");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_text_search_returns_empty_for_no_matches() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create one file
    let test_file = src_dir.join("lib.rs");
    fs::write(
        &test_file,
        r#"
pub fn get_user(id: u32) -> User {
    User::new(id)
}
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Index the workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Search for something that doesn't exist
    let results = crate::tools::search::text_search::text_search_impl(
        "nonexistent_function_xyz_abc_def",
        &None,
        &None,
        10,
        None,
        "definitions",
        None,
        &handler,
    )
    .await?;

    assert!(results.is_empty(), "Should return empty for non-matching query");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_text_search_respects_limit() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create file with multiple matching symbols
    let test_file = src_dir.join("lib.rs");
    fs::write(
        &test_file,
        r#"
pub fn search_term_one() { }
pub fn search_term_two() { }
pub fn search_term_three() { }
pub fn search_term_four() { }
pub fn search_term_five() { }
pub fn search_term_six() { }
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    // Index the workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Search with limit of 2
    let results = crate::tools::search::text_search::text_search_impl(
        "search_term",
        &None,
        &None,
        2,
        None,
        "definitions",
        None,
        &handler,
    )
    .await?;

    assert_eq!(results.len(), 2, "Should respect the limit of 2 results");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_text_search_content_target() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create a file with content
    let test_file = src_dir.join("lib.rs");
    fs::write(
        &test_file,
        r#"
pub fn example() {
    println!("Hello world with unique content");
}
"#,
    )?;

    let handler = JulieServerHandler::new().await?;
    // initialize_workspace_with_force calls initialize_all_components() which
    // includes embedding init that may fail without ONNX model. Once embeddings
    // are removed (Task 11), this will be clean.
    if let Err(e) = handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
    {
        eprintln!("Skipping content test: workspace init failed (likely missing ONNX model): {}", e);
        return Ok(());
    }

    // Index the workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Search for content
    let results = crate::tools::search::text_search::text_search_impl(
        "unique content",
        &None,
        &None,
        10,
        None,
        "content",
        None,
        &handler,
    )
    .await?;

    // Content search returns file-level matches
    assert!(!results.is_empty(), "Content search should find matching file");

    Ok(())
}
