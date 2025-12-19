//! TDD Tests for Semantic Search Filtering Under-Delivery
//!
//! **Finding #5: Query Filtering Can Under-Deliver**
//!
//! Problem:
//! - `semantic_search_impl()` requests `(limit * 5).min(200)` candidates from HNSW
//! - Filters for language/file_pattern AFTER fetching
//! - If user requests `limit=10` with `language="rust"` but most candidates are TypeScript:
//!   - HNSW returns 50 candidates (10 × 5)
//!   - Filtering discards 45 TypeScript symbols
//!   - Only 5 Rust symbols returned instead of 10
//!
//! Impact:
//! - Confusing UX: "Why did I only get 3 results when I asked for 10?"
//! - Reduces utility of language/file_pattern filters
//! - More matching symbols exist deeper in the pool but weren't fetched
//!
//! Solution:
//! - Dynamic widening: retry with larger search_limit if filtering under-delivers
//! - Cap at MAX_ATTEMPTS=3 to prevent runaway loops
//! - Add logging when widening occurs
//!
//! Test Scenarios:
//! 1. Semantic search with language filter delivers full limit
//! 2. Semantic search with file_pattern filter delivers full limit
//! 3. Dynamic widening retries with larger limits when needed
//! 4. Warning logged when max attempts reached without full delivery

use crate::extractors::Symbol;
use crate::handler::JulieServerHandler;
use crate::tools::search::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, StructuredContentExt};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

/// Test 1: Semantic search with language filter should deliver full limit
///
/// Given: Workspace with 100 TypeScript files and 20 Rust files
/// When: Semantic search for generic query with language="rust" and limit=10
/// Expected: Returns exactly 10 Rust symbols
/// Actual (BUG): Returns only 2-3 Rust symbols because most candidates are TypeScript
#[tokio::test]
#[ignore = "Failing test - reproduces Finding #5"]
async fn test_semantic_filtering_delivers_full_limit() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create TypeScript files (dominant in HNSW results)
    for i in 1..=100 {
        let file = workspace_path.join(format!("typescript_{}.ts", i));
        fs::write(
            &file,
            format!(
                r#"
export function processDataFunction{}() {{
    const result = fetchUserData();
    return result;
}}

export function transformDataFunction{}() {{
    const data = getApiData();
    return data;
}}
"#,
                i, i
            ),
        )?;
    }

    // Create Rust files (minority in HNSW results, but target of filter)
    for i in 1..=20 {
        let file = workspace_path.join(format!("rust_{}.rs", i));
        fs::write(
            &file,
            format!(
                r#"
pub fn process_data_function_{}() {{
    let result = fetch_user_data();
    result
}}

pub fn transform_data_function_{}() {{
    let data = get_api_data();
    data
}}
"#,
                i, i
            ),
        )?;
    }

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path, true).await?;

    // Wait for indexing + background embeddings (HNSW build)
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Semantic search with language filter
    // Query: generic "process data" (matches both TypeScript and Rust)
    // Filter: language="rust" to filter out TypeScript
    // Limit: 10
    let results = semantic_search_with_language(&handler, "process data", "rust", 10).await?;

    // BUG REPRODUCTION: This assertion WILL FAIL
    // Expected: 10 Rust symbols (there are 20 available)
    // Actual: ~2-3 Rust symbols (because HNSW fetched 50 candidates, mostly TypeScript)
    assert_eq!(
        results.len(),
        10,
        "BUG: Should return exactly 10 Rust symbols, got {}. \
         This happens because HNSW fetched (10 * 5 = 50) candidates, \
         but most were TypeScript and got filtered out.",
        results.len()
    );

    // Verify all results are actually Rust
    for symbol in &results {
        assert_eq!(
            symbol.language.to_lowercase(),
            "rust",
            "All results should be Rust symbols after language filter"
        );
    }

    Ok(())
}

/// Test 2: Semantic search with file_pattern filter should deliver full limit
///
/// Given: Workspace with files in multiple directories
/// When: Semantic search with file_pattern="src/**/*.rs" and limit=10
/// Expected: Returns exactly 10 symbols from src/ directory
#[tokio::test]
#[ignore = "Failing test - reproduces Finding #5 with file_pattern"]
async fn test_semantic_filtering_file_pattern_delivers_full_limit() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create files in tests/ directory (dominant in results)
    fs::create_dir_all(workspace_path.join("tests"))?;
    for i in 1..=100 {
        let file = workspace_path.join(format!("tests/test_{}.rs", i));
        fs::write(
            &file,
            format!(
                r#"
#[test]
fn test_process_data_{}() {{
    let result = process_data();
    assert!(result.is_ok());
}}
"#,
                i
            ),
        )?;
    }

    // Create files in src/ directory (target of filter)
    fs::create_dir_all(workspace_path.join("src"))?;
    for i in 1..=20 {
        let file = workspace_path.join(format!("src/module_{}.rs", i));
        fs::write(
            &file,
            format!(
                r#"
pub fn process_data_{}() {{
    let result = fetch_data();
    result
}}
"#,
                i
            ),
        )?;
    }

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path, true).await?;

    // Wait for indexing + background embeddings
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Semantic search with file_pattern filter
    let results = semantic_search_with_pattern(&handler, "process data", "src/**/*.rs", 10).await?;

    // BUG REPRODUCTION: This assertion WILL FAIL
    // Expected: 10 symbols from src/ directory (there are 20 available)
    // Actual: ~2-3 symbols (because HNSW fetched 50 candidates, mostly from tests/)
    assert_eq!(
        results.len(),
        10,
        "BUG: Should return exactly 10 symbols from src/, got {}",
        results.len()
    );

    // Verify all results match the pattern
    for symbol in &results {
        assert!(
            symbol.file_path.starts_with("src/"),
            "All results should be from src/ directory after file_pattern filter"
        );
    }

    Ok(())
}

/// Test 3: Dynamic widening should retry with larger limits
///
/// This test verifies that when filtering under-delivers, the implementation
/// retries with progressively larger search limits.
#[tokio::test]
#[ignore = "Pending implementation - tests dynamic widening behavior"]
async fn test_dynamic_widening_retries() -> Result<()> {
    // This test will verify that:
    // 1. First attempt: search_limit = limit * 5 (e.g., 10 * 5 = 50)
    // 2. If under-delivered: search_limit = 100 (doubled)
    // 3. If still under-delivered: search_limit = 200 (doubled again)
    // 4. Max 3 attempts before giving up

    // Implementation note: This requires inspecting logs or adding
    // instrumentation to verify retry behavior.

    Ok(())
}

// ============================================================================
// Test Helper Functions
// ============================================================================

async fn create_test_handler(workspace_path: &Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;
    Ok(handler)
}

async fn index_workspace(
    handler: &JulieServerHandler,
    workspace_path: &Path,
    force: bool,
) -> Result<()> {
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(force),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    index_tool.call_tool(handler).await?;
    Ok(())
}

/// Perform semantic search with language filter
async fn semantic_search_with_language(
    handler: &JulieServerHandler,
    query: &str,
    language: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    let search_tool = FastSearchTool {
        query: query.to_string(),
        search_method: "semantic".to_string(),
        limit,
        search_target: "content".to_string(),
        file_pattern: None,
        language: Some(language.to_string()),
        context_lines: None,
        output_format: None,
        workspace: None,
        output: None,
    };

    let result_json = search_tool.call_tool(handler).await?;

    // Parse JSON result to extract symbols
    parse_search_results(&result_json)
}

/// Perform semantic search with file_pattern filter
async fn semantic_search_with_pattern(
    handler: &JulieServerHandler,
    query: &str,
    pattern: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    let search_tool = FastSearchTool {
        query: query.to_string(),
        search_method: "semantic".to_string(),
        limit,
        search_target: "content".to_string(),
        file_pattern: Some(pattern.to_string()),
        language: None,
        context_lines: None,
        output_format: None,
        workspace: None,
        output: None,
    };

    let result_json = search_tool.call_tool(handler).await?;

    // Parse JSON result to extract symbols
    parse_search_results(&result_json)
}

/// Extract symbols from CallToolResult
fn parse_search_results(result: &CallToolResult) -> Result<Vec<Symbol>> {
    // Extract "results" from structured_content
    let symbols = result
        .structured_content()
        .and_then(|map| map.get("results").cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| anyhow::anyhow!("No results in CallToolResult"))?;

    Ok(symbols)
}
