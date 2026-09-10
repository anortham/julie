use crate::tests::helpers::mcp::call_tool_result_text as extract_text_from_result;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::search::FastSearchTool;
use anyhow::Result;
use std::fs;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_basic() -> Result<()> {
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

    let handler = snapshot_context(&workspace_path)?;

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

    let handler = snapshot_context(&workspace_path)?;

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

    let unknown_workspace = julie_test_support::FakeToolContext::new()
        .with_workspace_id("snapshot-fixture")
        .with_resolved_target(julie_context::WorkspaceTarget::Target(
            "nonexistent_workspace_id".to_string(),
        ));
    let result = search_invalid.call_tool(&unknown_workspace).await;
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

    let handler = snapshot_context(&workspace_path)?;

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
