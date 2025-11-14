//! TDD Tests for Vector Store Reload Mechanism
//!
//! **Bug #2: Vector Store Never Refreshed After Rebuilds**
//!
//! Problem:
//! - `handler.rs:ensure_vector_store()` early-returns if vector store exists
//! - `embeddings.rs:build_and_save_hnsw_index()` builds HNSW, saves to disk, then drops it
//! - In-memory vector store stays frozen even after background rebuild completes
//! - Users must restart Julie to see updated semantic search results
//!
//! Solution:
//! - Add timestamp/version tracking to detect when on-disk HNSW is newer
//! - Reload vector store from disk when staleness detected
//!
//! Test Scenarios:
//! 1. Vector store reloads after manual reindex (without restart)
//! 2. Semantic search returns fresh results after reload
//! 3. Concurrent searches don't crash during reload
//! 4. Reload mechanism detects on-disk timestamp changes

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

/// Test 1: Vector store reloads after manual reindex
///
/// Given: Workspace indexed with initial file
/// When: Add new file, trigger manual reindex, perform semantic search
/// Expected: Semantic search includes new file WITHOUT restart
/// Actual (BUG): Semantic search returns stale results (missing new file)
#[tokio::test]
#[ignore = "Failing test - reproduces Bug #2"]
async fn test_vector_store_reloads_after_manual_reindex() -> Result<()> {
    // Skip embeddings env var doesn't help here - we need real HNSW for this test
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // 1. Create initial file and index workspace
    let initial_file = workspace_path.join("initial.rs");
    fs::write(&initial_file, "fn initial_function() { println!(\"hello\"); }")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path, true).await?;

    // Wait for background embedding job to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 2. Perform baseline semantic search
    let baseline_results = semantic_search(&handler, "initial function", 10).await?;
    assert!(
        !baseline_results.is_empty(),
        "Baseline search should find initial_function"
    );

    // 3. Add NEW file that wasn't in original index
    let new_file = workspace_path.join("new.rs");
    fs::write(&new_file, "fn new_function() { println!(\"world\"); }")?;

    // 4. Trigger manual reindex (force=true)
    index_workspace(&handler, workspace_path, true).await?;

    // Wait for background HNSW rebuild to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // 5. Semantic search WITHOUT restarting Julie
    let updated_results = semantic_search(&handler, "new function", 10).await?;

    // BUG REPRODUCTION: This assertion WILL FAIL
    // The new file should be findable, but vector store is frozen
    assert!(
        !updated_results.is_empty(),
        "BUG: Semantic search should find new_function after reindex (without restart)"
    );

    // Verify the new function is actually in the database (FTS5 should find it)
    let text_results = text_search(&handler, "new_function", 10).await?;
    assert!(
        !text_results.is_empty(),
        "Text search DOES find new_function (proves it's in database)"
    );

    Ok(())
}

/// Test 2: HNSW files on disk are newer than in-memory load time
///
/// Given: Vector store loaded from disk
/// When: Background job saves new HNSW to disk
/// Expected: Staleness detection triggers reload
#[tokio::test]
#[ignore = "Not yet implemented - part of Bug #2 fix"]
async fn test_detects_newer_hnsw_files_on_disk() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create and index workspace
    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn test() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path, true).await?;

    // Wait for initial HNSW build
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Get initial vector store load time
    let initial_load_time = get_vector_store_load_time(&handler).await?;

    // Sleep to ensure filesystem timestamp changes
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Trigger background HNSW rebuild (simulates manual reindex)
    index_workspace(&handler, workspace_path, true).await?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Check if staleness is detected
    let is_stale = check_vector_store_staleness(&handler).await?;

    assert!(
        is_stale,
        "Vector store should be detected as stale after on-disk HNSW rebuild"
    );

    // Verify new load time is newer than initial
    let new_load_time = get_vector_store_load_time(&handler).await?;
    assert!(
        new_load_time > initial_load_time,
        "Vector store should have reloaded with newer timestamp"
    );

    Ok(())
}

/// Test 3: Concurrent semantic searches don't crash during reload
///
/// Given: Vector store is reloading
/// When: Multiple concurrent semantic searches execute
/// Expected: All searches succeed (either on old or new vector store)
#[tokio::test]
#[ignore = "Not yet implemented - part of Bug #2 fix"]
async fn test_concurrent_searches_during_reload() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create test files
    for i in 0..10 {
        let file = workspace_path.join(format!("file_{}.rs", i));
        fs::write(&file, format!("fn function_{}() {{}}", i))?;
    }

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path, true).await?;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    // Spawn multiple concurrent searches
    let mut search_handles = vec![];
    for i in 0..5 {
        let handler_clone = handler.clone();
        let handle = tokio::spawn(async move {
            semantic_search(&handler_clone, &format!("function {}", i), 5).await
        });
        search_handles.push(handle);
    }

    // Trigger reload while searches are running
    index_workspace(&handler, workspace_path, true).await?;

    // Wait for all searches to complete
    for handle in search_handles {
        let result = handle.await?;
        assert!(result.is_ok(), "Concurrent search should not crash during reload");
    }

    Ok(())
}

// ============================================================================
// Test Helpers
// ============================================================================

use crate::extractors::base::Symbol;
use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;

async fn create_test_handler(workspace_path: &std::path::Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;
    Ok(handler)
}

async fn index_workspace(
    handler: &JulieServerHandler,
    workspace_path: &std::path::Path,
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

/// Perform semantic search using fast_search tool
async fn semantic_search(
    handler: &JulieServerHandler,
    query: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    use crate::tools::search::FastSearchTool;

    let search_tool = FastSearchTool {
        query: query.to_string(),
        search_method: "semantic".to_string(),
        limit,
        search_target: "content".to_string(),
        file_pattern: None,
        language: None,
        context_lines: None,
        workspace: None,
        output: None,
    };

    let result = search_tool.call_tool(handler).await?;

    // Parse JSON result to extract symbols
    // For now, assume empty vec (actual parsing TBD)
    // TODO: Implement proper JSON parsing when test runs
    Ok(Vec::new())
}

/// Perform text search using fast_search tool
async fn text_search(
    handler: &JulieServerHandler,
    query: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    use crate::tools::search::FastSearchTool;

    let search_tool = FastSearchTool {
        query: query.to_string(),
        search_method: "text".to_string(),
        limit,
        search_target: "content".to_string(),
        file_pattern: None,
        language: None,
        context_lines: None,
        workspace: None,
        output: None,
    };

    let result = search_tool.call_tool(handler).await?;

    // Parse JSON result to extract symbols
    // For now, assume empty vec (actual parsing TBD)
    // TODO: Implement proper JSON parsing when test runs
    Ok(Vec::new())
}

/// Get vector store load time (to detect staleness)
/// Returns SystemTime of when vector store was loaded from disk
async fn get_vector_store_load_time(
    _handler: &JulieServerHandler,
) -> Result<std::time::SystemTime> {
    // TODO: Implement this when VectorStore has load_time field
    // For now, return current time as placeholder
    Ok(std::time::SystemTime::now())
}

/// Check if vector store is stale (on-disk HNSW is newer than in-memory)
async fn check_vector_store_staleness(_handler: &JulieServerHandler) -> Result<bool> {
    // TODO: Implement staleness check
    // Compare on-disk HNSW file mtime with vector store load_time
    // For now, return false as placeholder
    Ok(false)
}
