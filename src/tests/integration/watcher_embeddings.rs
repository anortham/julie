//! TDD Tests for File Watcher Embeddings Persistence
//!
//! **Bug #1: File Watcher Embeddings Never Persisted**
//!
//! Problem:
//! - `handlers.rs:146` calls `embed_symbols_batch()` which returns embeddings
//! - Line 147: `Ok(_) =>` discards the embeddings completely
//! - Embeddings are NEVER persisted to SQLite database
//! - HNSW index is NEVER updated after file changes
//! - Result: File edits are invisible to semantic search
//!
//! Solution:
//! - Persist embeddings returned from `embed_symbols_batch()` to SQLite
//! - Trigger HNSW index rebuild after file changes
//! - Ensure semantic search sees updated code immediately
//!
//! Test Scenarios:
//! 1. File edit updates semantic search results (without manual reindex)
//! 2. Multiple file edits accumulate in semantic search
//! 3. File deletion removes symbols from semantic search
//! 4. Embeddings persist across Julie restarts

use anyhow::Result;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

/// Test 1: File edit updates semantic search WITHOUT manual reindex
///
/// Given: Workspace indexed with initial file
/// When: Edit file via file watcher, perform semantic search
/// Expected: Semantic search finds NEW content immediately
/// Actual (BUG): Semantic search returns stale results (missing new content)
#[tokio::test]
#[ignore = "Failing test - reproduces Bug #1"]
async fn test_file_edit_updates_semantic_search() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // 1. Create initial file and index workspace
    let test_file = workspace_path.join("user.rs");
    fs::write(
        &test_file,
        r#"
pub struct User {
    pub id: u64,
    pub name: String,
}

impl User {
    pub fn get_user_by_id(id: u64) -> Option<User> {
        // Initial implementation
        None
    }
}
"#,
    )?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path, true).await?;

    // Wait for initial indexing + background embeddings
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 2. Baseline semantic search - should find "get user by id"
    let baseline_results = semantic_search(&handler, "get user by id", 10).await?;
    assert!(
        !baseline_results.is_empty(),
        "Baseline search should find get_user_by_id function"
    );

    // 3. EDIT the file - add new function via file watcher simulation
    fs::write(
        &test_file,
        r#"
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,  // New field
}

impl User {
    pub fn get_user_by_id(id: u64) -> Option<User> {
        // Initial implementation
        None
    }

    // NEW FUNCTION ADDED
    pub fn create_new_user(name: String, email: String) -> User {
        User {
            id: 0,
            name,
            email,
        }
    }
}
"#,
    )?;

    // 4. Trigger file watcher handler (simulates inotify/FSEvents)
    trigger_file_watcher(&handler, &test_file).await?;

    // Wait for incremental indexing + background embeddings
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 5. Semantic search for NEW content WITHOUT manual reindex
    let updated_results = semantic_search(&handler, "create new user", 10).await?;

    // BUG REPRODUCTION: This assertion WILL FAIL
    // The new function should be findable, but embeddings were never persisted
    assert!(
        !updated_results.is_empty(),
        "BUG: Semantic search should find create_new_user after file edit (without reindex)"
    );

    // Verify the new function is in the database (FTS5 should find it)
    let text_results = text_search(&handler, "create_new_user", 10).await?;
    assert!(
        !text_results.is_empty(),
        "Text search DOES find create_new_user (proves symbols were extracted)"
    );

    Ok(())
}

/// Test 2: Multiple file edits accumulate in semantic search
///
/// Given: Workspace with multiple files
/// When: Edit multiple files sequentially
/// Expected: Each edit is immediately findable via semantic search
#[tokio::test]
#[ignore = "Not yet implemented - part of Bug #1 fix"]
async fn test_multiple_edits_accumulate_in_semantic_search() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create 3 files
    for i in 1..=3 {
        let file = workspace_path.join(format!("file_{}.rs", i));
        fs::write(&file, format!("fn initial_function_{}() {{}}", i))?;
    }

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path, true).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Edit each file sequentially, adding new functions
    for i in 1..=3 {
        let file = workspace_path.join(format!("file_{}.rs", i));
        fs::write(
            &file,
            format!(
                "fn initial_function_{}() {{}}\nfn new_function_{}() {{}}",
                i, i
            ),
        )?;

        trigger_file_watcher(&handler, &file).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Each new function should be immediately findable
        let results = semantic_search(&handler, &format!("new function {}", i), 10).await?;
        assert!(
            !results.is_empty(),
            "New function {} should be findable after edit",
            i
        );
    }

    Ok(())
}

/// Test 3: File deletion removes symbols from semantic search
///
/// Given: Workspace with indexed file
/// When: Delete file via file watcher
/// Expected: Symbols are removed from semantic search
#[tokio::test]
#[ignore = "Not yet implemented - part of Bug #1 fix"]
async fn test_file_deletion_updates_semantic_search() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    let test_file = workspace_path.join("temp.rs");
    fs::write(&test_file, "fn temporary_function() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path, true).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Verify function is findable
    let before_delete = semantic_search(&handler, "temporary function", 10).await?;
    assert!(
        !before_delete.is_empty(),
        "Should find function before delete"
    );

    // Delete file and trigger watcher
    fs::remove_file(&test_file)?;
    trigger_file_deletion(&handler, &test_file).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Function should no longer be findable
    let after_delete = semantic_search(&handler, "temporary function", 10).await?;
    assert!(
        after_delete.is_empty(),
        "Should NOT find function after delete"
    );

    Ok(())
}

// ============================================================================
// Test Helpers
// ============================================================================

use crate::extractors::base::Symbol;
use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;
use std::path::Path;

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

/// Trigger file watcher handler for modified file
async fn trigger_file_watcher(_handler: &JulieServerHandler, file_path: &Path) -> Result<()> {
    // Use the same handler that file watcher would call
    use crate::database::SymbolDatabase;
    use crate::embeddings::EmbeddingEngine;
    use crate::extractors::ExtractorManager;
    use crate::watcher::handlers::handle_file_created_or_modified_static;
    use std::sync::{Arc, Mutex};
    use tokio::sync::RwLock;

    // Get workspace root from file path (go up one level from test file)
    let workspace_root = file_path.parent().unwrap();

    // Create components directly (like existing watcher tests do)
    let db_path = workspace_root.join(".julie/db/symbols.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path)?));

    let extractor_manager = Arc::new(ExtractorManager::new());
    let embeddings = Arc::new(RwLock::new(None::<EmbeddingEngine>));

    handle_file_created_or_modified_static(
        file_path.to_path_buf(),
        &db,
        &embeddings,
        &extractor_manager,
        None,
        workspace_root,
    )
    .await?;

    Ok(())
}

/// Trigger file watcher handler for deleted file
async fn trigger_file_deletion(_handler: &JulieServerHandler, file_path: &Path) -> Result<()> {
    // Simulate file deletion event
    use crate::database::SymbolDatabase;
    use crate::watcher::handlers::handle_file_deleted_static;
    use std::sync::{Arc, Mutex};

    // Get workspace root from file path
    let workspace_root = file_path.parent().unwrap();

    // Create components directly
    let db_path = workspace_root.join(".julie/db/symbols.db");
    let db = Arc::new(Mutex::new(SymbolDatabase::new(&db_path)?));

    handle_file_deleted_static(file_path.to_path_buf(), &db, None, workspace_root).await?;

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
        output_format: None,
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
async fn text_search(handler: &JulieServerHandler, query: &str, limit: u32) -> Result<Vec<Symbol>> {
    use crate::tools::search::FastSearchTool;

    let search_tool = FastSearchTool {
        query: query.to_string(),
        search_method: "text".to_string(),
        limit,
        search_target: "content".to_string(),
        file_pattern: None,
        language: None,
        context_lines: None,
        output_format: None,
        workspace: None,
        output: None,
    };

    let result = search_tool.call_tool(handler).await?;

    // Parse JSON result to extract symbols
    // For now, assume empty vec (actual parsing TBD)
    // TODO: Implement proper JSON parsing when test runs
    Ok(Vec::new())
}
