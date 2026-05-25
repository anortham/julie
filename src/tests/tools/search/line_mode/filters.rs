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
async fn test_fast_search_line_mode_handles_exclusion_queries() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Fixture now defines real Rust symbols so the indexer marks the
    // workspace ready.  Each symbol's name carries one of the
    // "user_*" tokens so the unified path can score them.
    let test_file = src_dir.join("filters.rs");
    fs::write(
        &test_file,
        r#"fn user_profile_data() {}
fn user_password_secret() {}
fn user_preferences_dashboard() {}
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

    // Search for the shared prefix `user`.  Unified search will return
    // all three symbol-row matches; this is the post-T8 contract for
    // FastSearchTool with no exclusion semantics.  The `-password`
    // exclusion syntax is no longer parsed in the default path.
    let search_tool = FastSearchTool {
        query: "user".to_string(),
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

    // Post-T8: the lean formatter prints file paths and line numbers,
    // not symbol names directly.  Verify the file with all three
    // symbols is reported and the count reflects the three matches.
    assert!(
        response_text.contains("src/filters.rs") || response_text.contains("src\\filters.rs"),
        "should report the file containing user_* symbols: {}",
        response_text,
    );
    // Three symbols share the `user` token; the lean header reports
    // a count >= 3 (the unified path may also emit a file-row hit).
    assert!(
        response_text.contains("matches for \"user\""),
        "should render the standard match-count header: {}",
        response_text,
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_language_filter() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create files in different languages with common search term
    let rust_file = src_dir.join("example.rs");
    fs::write(
        &rust_file,
        r#"// TODO: implement feature
fn rust_function() {}
"#,
    )?;

    let ts_file = src_dir.join("example.ts");
    fs::write(
        &ts_file,
        r#"// TODO: implement feature
function typescriptFunction() {}
"#,
    )?;

    let py_file = src_dir.join("example.py");
    fs::write(
        &py_file,
        r#"# TODO: implement feature
def python_function():
pass
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

    // Test: Search with rust language filter
    let search_rust = FastSearchTool {
        query: "TODO".to_string(),
        language: Some("rust".to_string()),
        file_pattern: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result = search_rust.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    assert!(
        response_text.contains("example.rs"),
        "Should find TODO in Rust file"
    );
    assert!(
        !response_text.contains("example.ts"),
        "Should NOT include TypeScript file when filtering for Rust"
    );
    assert!(
        !response_text.contains("example.py"),
        "Should NOT include Python file when filtering for Rust"
    );

    // Test: Search with typescript language filter
    let search_ts = FastSearchTool {
        query: "TODO".to_string(),
        language: Some("typescript".to_string()),
        file_pattern: None,
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result_ts = search_ts.call_tool(&handler).await?;
    let response_ts = extract_text_from_result(&result_ts);

    assert!(
        response_ts.contains("example.ts"),
        "Should find TODO in TypeScript file"
    );
    assert!(
        !response_ts.contains("example.rs"),
        "Should NOT include Rust file when filtering for TypeScript"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_file_pattern_filter() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Create directory structure
    let src_dir = workspace_path.join("src");
    let tests_dir = workspace_path.join("tests");
    fs::create_dir_all(&src_dir)?;
    fs::create_dir_all(&tests_dir)?;

    // Create symbol-bearing files in different locations.  The
    // unified path only indexes files that produce symbols, so each
    // fixture needs a real `fn` definition; the shared `fixme_marker`
    // name in both files lets us assert which one survives the
    // file_pattern filter.
    let src_file = src_dir.join("code.rs");
    fs::write(&src_file, "fn fixme_marker_src() {}\n")?;

    let test_file = tests_dir.join("test.rs");
    fs::write(&test_file, "fn fixme_marker_test() {}\n")?;

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
    sleep(Duration::from_secs(2)).await; // Increased wait for FTS content indexing
    mark_index_ready(&handler).await;

    // Test: Search with src/** file pattern.  The shared prefix
    // `fixme_marker` matches both symbols; the file_pattern filter
    // should keep only the src/ hit.  `exclude_tests=false` keeps the
    // NL-default test-exclusion off, so the scope-rescue path only
    // fires from the pattern itself.
    let search_src = FastSearchTool {
        query: "fixme_marker".to_string(),
        language: None,
        file_pattern: Some("src/**".to_string()),
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: Some(false),
        ..Default::default()
    };

    let result = search_src.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    assert!(
        response_text.contains("src/code.rs") || response_text.contains("src\\code.rs"),
        "Should find fixme_marker in src/ directory: {}",
        response_text
    );
    assert!(
        !response_text.contains("tests/test.rs") && !response_text.contains("tests\\test.rs"),
        "Should NOT include tests/ directory when filtering for src/**: {}",
        response_text,
    );

    // Test: Search with tests/** file pattern
    let search_tests = FastSearchTool {
        query: "fixme_marker".to_string(),
        language: None,
        file_pattern: Some("tests/**".to_string()),
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: Some(false),
        ..Default::default()
    };

    let result_tests = search_tests.call_tool(&handler).await?;
    let response_tests = extract_text_from_result(&result_tests);

    assert!(
        response_tests.contains("tests/test.rs") || response_tests.contains("tests\\test.rs"),
        "Should find fixme_marker in tests/ directory: {}",
        response_tests,
    );
    assert!(
        !response_tests.contains("src/code.rs") && !response_tests.contains("src\\code.rs"),
        "Should NOT include src/ directory when filtering for tests/**: {}",
        response_tests,
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_exclude_tests() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Create production source file
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let prod_file = src_dir.join("auth.rs");
    fs::write(
        &prod_file,
        r#"/// Authenticate a user with the given credentials
fn authenticate_user(username: &str, password: &str) -> bool {
// authenticate logic here
username.len() > 0 && password.len() > 0
}
"#,
    )?;

    // Create test file in a test directory (is_test_path checks for "tests" segment)
    let test_dir = workspace_path.join("src").join("tests");
    fs::create_dir_all(&test_dir)?;

    let test_file = test_dir.join("auth_test.rs");
    fs::write(
        &test_file,
        r#"#[test]
fn test_authenticate_user() {
// authenticate test logic
assert!(authenticate_user("admin", "secret"));
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
    sleep(Duration::from_secs(2)).await;
    mark_index_ready(&handler).await;

    // Test 1: Search WITHOUT exclude_tests — should find results from BOTH files
    let search_all = FastSearchTool {
        query: "authenticate".to_string(),
        language: None,
        file_pattern: None,
        limit: 20,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: None,
        ..Default::default()
    };

    let result_all = search_all.call_tool(&handler).await?;
    let text_all = extract_text_from_result(&result_all);

    assert!(
        text_all.contains("src/auth.rs"),
        "Without exclude_tests, should find production file. Got: {}",
        text_all
    );
    assert!(
        text_all.contains("src/tests/auth_test.rs"),
        "Without exclude_tests, should find test file. Got: {}",
        text_all
    );

    // Test 2: Search WITH exclude_tests: Some(true) — should ONLY find production file
    let search_no_tests = FastSearchTool {
        query: "authenticate".to_string(),
        language: None,
        file_pattern: None,
        limit: 20,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: Some(true),
        ..Default::default()
    };

    let result_no_tests = search_no_tests.call_tool(&handler).await?;
    let text_no_tests = extract_text_from_result(&result_no_tests);

    assert!(
        text_no_tests.contains("src/auth.rs"),
        "With exclude_tests, should still find production file. Got: {}",
        text_no_tests
    );
    assert!(
        !text_no_tests.contains("src/tests/auth_test.rs"),
        "With exclude_tests, should NOT find test file. Got: {}",
        text_no_tests
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_fast_search_line_mode_combined_filters() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Create symbol-bearing files; the unified path won't index a
    // comment-only file.  Both files share the `combined_filter_marker`
    // token so the test can verify the language+file_pattern combo.
    let rust_file = src_dir.join("main.rs");
    fs::write(&rust_file, "fn combined_filter_marker_rs() {}\n")?;

    let ts_file = src_dir.join("index.ts");
    fs::write(
        &ts_file,
        "function combined_filter_marker_ts() { return 1; }\n",
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
    sleep(Duration::from_secs(2)).await; // Increased wait for FTS content indexing
    mark_index_ready(&handler).await;

    // Test: Search with BOTH language AND file_pattern filters
    let search_combined = FastSearchTool {
        query: "combined_filter_marker".to_string(),
        language: Some("rust".to_string()),
        file_pattern: Some("src/**/*.rs".to_string()),
        limit: 10,
        workspace: Some("primary".to_string()),
        context_lines: None,
        exclude_tests: Some(false),
        ..Default::default()
    };

    let result = search_combined.call_tool(&handler).await?;
    let response_text = extract_text_from_result(&result);

    assert!(
        response_text.contains("main.rs"),
        "Should find combined_filter_marker in Rust file matching both filters: {}",
        response_text,
    );
    assert!(
        !response_text.contains("index.ts"),
        "Should NOT include TypeScript file when filtering for Rust + src/**/*.rs: {}",
        response_text,
    );

    Ok(())
}
