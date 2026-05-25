use super::mark_index_ready;
use crate::handler::JulieServerHandler;
use crate::tests::helpers::mcp::call_tool_result_text as extract_text_from_result;
use crate::tools::search::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;
use tokio::time::{Duration, sleep};

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_basic() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Create test file with known content
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("example.rs");
    fs::write(
        &test_file,
        r#"// TODO: implement authentication
fn getUserData() {
// TODO: add validation
println!("Getting user data");
}

fn processPayment() {
// This function is complete
println!("Processing payment");
}
"#,
    )?;

    // Initialize handler and index
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    sleep(Duration::from_millis(500)).await;
    mark_index_ready(&handler).await;

    // Post-T8: the unified path searches indexed symbol fields (name,
    // signature, doc_comment, code_body, etc.).  Plain "//" line
    // comments that live outside any symbol body are NOT in the index,
    // so the legacy "find every TODO comment" assertion no longer
    // describes how fast_search works.  We instead exercise the basic
    // unified content-search contract: searching for a symbol name
    // surfaces that symbol from the matching file.
    let search_tool = FastSearchTool {
        query: "getUserData".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search_tool.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    assert!(
        response_text.contains("getUserData"),
        "unified search should find the symbol by name: {}",
        response_text,
    );
    assert!(
        response_text.contains("example.rs"),
        "unified search should report the matching file: {}",
        response_text,
    );
    assert!(
        !response_text.contains("processPayment"),
        "unified search should not surface unrelated symbols: {}",
        response_text,
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_respects_workspace_filter() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create two files with distinct content
    let file1 = src_dir.join("module_a.rs");
    fs::write(
        &file1,
        "fn function_alpha() { println!(\"alpha_marker\"); }\n",
    )?;

    let file2 = src_dir.join("module_b.rs");
    fs::write(
        &file2,
        "fn function_beta() { println!(\"beta_marker\"); }\n",
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;
    sleep(Duration::from_millis(500)).await;
    mark_index_ready(&handler).await;

    // Test 1: Search primary workspace explicitly - should find results
    let search_primary = FastSearchTool {
        query: "function_alpha".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search_primary.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    // Post-T8: the unified path returns symbol-row matches.  Search for
    // a symbol name (`function_alpha`) so the assertion exercises the
    // index field that actually carries the term (name field) rather
    // than the legacy line-mode "string literal in body" path.
    assert!(
        response_text.contains("function_alpha"),
        "Primary workspace search should find the matching symbol: {}",
        response_text
    );
    assert!(
        response_text.contains("module_a.rs"),
        "Primary workspace search should show correct file: {}",
        response_text
    );

    // Test 2: Search with invalid workspace ID - under stdio mode
    // (no daemon registry) the resolver silently accepts the unknown
    // id and the search returns the missing-index message instead of
    // erroring.  This matches the unified-path "no rescue, no result"
    // contract.
    let search_invalid = FastSearchTool {
        query: "function_alpha".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some("nonexistent_workspace_id".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search_invalid.call_tool(&handler).await;
    // Stdio mode silently accepts unknown workspace ids: the search
    // then either errors at the database probe (workspace dir missing)
    // OR returns the neutral missing-index text.  Either is acceptable
    // — both communicate that the workspace cannot be searched.
    match result {
        Err(_) => { /* daemon mode would surface "no such workspace" */ }
        Ok(call_result) => {
            let text = extract_text_from_result(&call_result);
            assert!(
                text.contains("Search requires a Tantivy index")
                    || text.contains("Workspace not indexed yet"),
                "Searching a non-existent workspace should report missing index: {}",
                text,
            );
        }
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_symbols_mode_default() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("example.rs");
    fs::write(
        &test_file,
        r#"pub fn getUserData() -> User {
User { name: "test" }
}
"#,
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    sleep(Duration::from_millis(500)).await;
    mark_index_ready(&handler).await;

    let search_tool = FastSearchTool {
        query: "getUserData".to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search_tool.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    assert!(
        response_text.contains("getUserData"),
        "Should find function symbol"
    );
    assert!(
        response_text.contains("getUserData")
            || response_text.contains("Found")
            || response_text.contains("symbol"),
        "Should show basic search result info"
    );

    Ok(())
}
