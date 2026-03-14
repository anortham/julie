//! Tests for Tantivy-based text search implementation
//!
//! Tests that text_search_impl() correctly uses Tantivy SearchIndex
//! for both symbol and content searches.

use anyhow::Result;
use serial_test::serial;
use std::fs;
use std::sync::Arc;
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

    let handler = JulieServerHandler::new_for_test().await?;
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
    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "get_user",
        &None,
        &None,
        10,
        None,
        "definitions",
        None,
        None,
        &handler,
    )
    .await?;

    assert!(!results.is_empty(), "Should find the function");
    assert_eq!(
        results[0].name, "get_user",
        "Should match the function name"
    );

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

    let handler = JulieServerHandler::new_for_test().await?;
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
    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "process_data",
        &Some("rust".to_string()),
        &None,
        10,
        None,
        "definitions",
        None,
        None,
        &handler,
    )
    .await?;

    assert!(!results.is_empty(), "Should find at least one Rust result");
    for result in &results {
        assert_eq!(
            result.language.to_lowercase(),
            "rust",
            "All results should be Rust"
        );
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

    let handler = JulieServerHandler::new_for_test().await?;
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
    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "helper",
        &None,
        &Some("src/**".to_string()),
        10,
        None,
        "definitions",
        None,
        None,
        &handler,
    )
    .await?;

    assert!(!results.is_empty(), "Should find results in src/");
    for result in &results {
        assert!(
            result.file_path.starts_with("src/"),
            "All results should be in src/"
        );
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

    let handler = JulieServerHandler::new_for_test().await?;
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
    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "nonexistent_function_xyz_abc_def",
        &None,
        &None,
        10,
        None,
        "definitions",
        None,
        None,
        &handler,
    )
    .await?;

    assert!(
        results.is_empty(),
        "Should return empty for non-matching query"
    );

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

    let handler = JulieServerHandler::new_for_test().await?;
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
    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "search_term",
        &None,
        &None,
        2,
        None,
        "definitions",
        None,
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

    let handler = JulieServerHandler::new_for_test().await?;
    // initialize_workspace_with_force calls initialize_all_components() which
    // includes embedding init that may fail without ONNX model. Once embeddings
    // are removed (Task 11), this will be clean.
    if let Err(e) = handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
    {
        eprintln!(
            "Skipping content test: workspace init failed (likely missing ONNX model): {}",
            e
        );
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
    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "unique content",
        &None,
        &None,
        10,
        None,
        "content",
        None,
        None,
        &handler,
    )
    .await?;

    // Content search returns file-level matches
    assert!(
        !results.is_empty(),
        "Content search should find matching file"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[serial(embedding_env)]
async fn test_nl_definition_search_can_enable_hybrid_without_prior_index_embedding() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    fs::write(
        src_dir.join("lib.rs"),
        r#"
pub fn lookup_user_profile(id: u32) -> String {
    format!("user-{id}")
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

    {
        let mut workspace_guard = handler.workspace.write().await;
        let workspace = workspace_guard
            .as_mut()
            .expect("workspace should be initialized");
        workspace.embedding_provider = None;
        workspace.embedding_runtime_status = None;
    }

    let _ = crate::tools::search::text_search::take_nl_definition_embedding_init_attempts(
        &workspace_path,
    );

    let query = "how should user profile lookups work".to_string();
    let start_barrier = Arc::new(tokio::sync::Barrier::new(3));

    let handler_a = handler.clone();
    let query_a = query.clone();
    let barrier_a = start_barrier.clone();
    let task_a = tokio::spawn(async move {
        barrier_a.wait().await;
        crate::tools::search::text_search::text_search_impl(
            &query_a,
            &None,
            &None,
            10,
            None,
            "definitions",
            None,
            None,
            &handler_a,
        )
        .await
    });

    let handler_b = handler.clone();
    let query_b = query;
    let barrier_b = start_barrier.clone();
    let task_b = tokio::spawn(async move {
        barrier_b.wait().await;
        crate::tools::search::text_search::text_search_impl(
            &query_b,
            &None,
            &None,
            10,
            None,
            "definitions",
            None,
            None,
            &handler_b,
        )
        .await
    });

    start_barrier.wait().await;

    let (result_a, result_b) = tokio::join!(task_a, task_b);
    let (results_a, _) = result_a??;
    let (results_b, _) = result_b??;

    assert!(
        results_a
            .iter()
            .any(|symbol| symbol.name == "lookup_user_profile"),
        "first NL definitions query should return symbol matches"
    );
    assert!(
        results_b
            .iter()
            .any(|symbol| symbol.name == "lookup_user_profile"),
        "second NL definitions query should return symbol matches"
    );

    let workspace_guard = handler.workspace.read().await;
    let workspace = workspace_guard
        .as_ref()
        .expect("workspace should still be initialized");
    assert!(
        workspace.embedding_runtime_status.is_some(),
        "NL definitions query should trigger deferred embedding init attempt"
    );

    let init_count = crate::tools::search::text_search::take_nl_definition_embedding_init_attempts(
        &workspace_path,
    );
    assert_eq!(
        init_count, 1,
        "concurrent NL definition queries should share one lazy init attempt"
    );

    Ok(())
}

/// Create a shared fixture with both production and test functions.
/// Returns (workspace_path, temp_dir) so the caller holds the TempDir alive.
async fn setup_workspace_with_test_and_prod_symbols(
) -> Result<(std::path::PathBuf, TempDir, crate::handler::JulieServerHandler)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    // Write a Rust file containing both production functions and annotated test functions.
    // The extractor detects #[test] attributes and sets metadata["is_test"] = true.
    fs::write(
        src_dir.join("payments.rs"),
        r#"
pub fn process_payment(amount: f64) -> bool {
    amount > 0.0
}

pub fn validate_input(data: &str) -> bool {
    !data.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_payment() {
        assert!(process_payment(10.0));
    }

    #[test]
    fn test_validate_input() {
        assert!(validate_input("hello"));
    }
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

    // Wait for background indexing to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    Ok((workspace_path, temp_dir, handler))
}

/// Test 1: When `exclude_tests: Some(true)` is set, test symbols are filtered from
/// definition search results. This exercises the `filter_test_symbols` path directly.
///
/// Note: The smart default for definition searches is always to include tests
/// (`search_target == "definitions"` → `exclude_tests = false`). The NL auto-exclude
/// smart default only resolves to `true` for non-definition targets, but those go
/// through `content_search_with_index` which operates on files, not symbols.
/// So explicit `Some(true)` is the practical mechanism for excluding tests.
#[tokio::test(flavor = "multi_thread")]
async fn test_exclude_tests_explicit_true_filters_test_symbols() -> Result<()> {
    let (_workspace_path, _temp_dir, handler) =
        setup_workspace_with_test_and_prod_symbols().await?;

    // Use an NL-like query so the intent is clear, but force exclude via Some(true)
    // because the smart default for definitions search is to always include tests.
    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "process payment",
        &None,
        &None,
        20,
        None,
        "definitions",
        None,
        Some(true), // explicitly exclude test symbols
        &handler,
    )
    .await?;

    let has_test_symbol = results.iter().any(|s| s.name == "test_process_payment");
    let has_prod_symbol = results.iter().any(|s| s.name == "process_payment");

    assert!(
        !has_test_symbol,
        "exclude_tests=true should filter out test_process_payment; results: {:?}",
        results.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert!(
        has_prod_symbol,
        "process_payment (production symbol) should still appear; results: {:?}",
        results.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    Ok(())
}

/// Test 2: `exclude_tests: Some(false)` overrides any smart default and includes test
/// symbols even when the query looks like natural language.
#[tokio::test(flavor = "multi_thread")]
async fn test_exclude_tests_explicit_override_includes_tests() -> Result<()> {
    let (_workspace_path, _temp_dir, handler) =
        setup_workspace_with_test_and_prod_symbols().await?;

    // Explicit Some(false) must include test symbols regardless of query shape.
    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "process payment",
        &None,
        &None,
        20,
        None,
        "definitions",
        None,
        Some(false), // explicit include — override any smart default
        &handler,
    )
    .await?;

    let has_test_symbol = results.iter().any(|s| s.name == "test_process_payment");

    assert!(
        has_test_symbol,
        "exclude_tests=false should include test_process_payment; results: {:?}",
        results.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    Ok(())
}

/// Test 3: The smart default (`exclude_tests: None`) for a definition search includes test
/// symbols — definition searches are never auto-filtered.
#[tokio::test(flavor = "multi_thread")]
async fn test_definition_search_includes_tests_by_default() -> Result<()> {
    let (_workspace_path, _temp_dir, handler) =
        setup_workspace_with_test_and_prod_symbols().await?;

    // Search directly by identifier name — not NL-like (single term / underscore).
    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "test_process_payment",
        &None,
        &None,
        20,
        None,
        "definitions",
        None,
        None, // smart default — should include tests for definition searches
        &handler,
    )
    .await?;

    let has_test_symbol = results.iter().any(|s| s.name == "test_process_payment");

    assert!(
        has_test_symbol,
        "definition search with exclude_tests=None should include test symbols; results: {:?}",
        results.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    Ok(())
}

/// Test 4: `exclude_tests: Some(true)` with a direct identifier search for a test function
/// should suppress that symbol even in definition mode.
#[tokio::test(flavor = "multi_thread")]
async fn test_exclude_tests_explicit_true_filters_for_definitions() -> Result<()> {
    let (_workspace_path, _temp_dir, handler) =
        setup_workspace_with_test_and_prod_symbols().await?;

    let (results, _relaxed) = crate::tools::search::text_search::text_search_impl(
        "test_process_payment",
        &None,
        &None,
        20,
        None,
        "definitions",
        None,
        Some(true), // force exclude even in definition mode
        &handler,
    )
    .await?;

    let has_test_symbol = results.iter().any(|s| s.name == "test_process_payment");

    assert!(
        !has_test_symbol,
        "exclude_tests=Some(true) should remove test_process_payment from definition results; \
         results: {:?}",
        results.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    Ok(())
}
