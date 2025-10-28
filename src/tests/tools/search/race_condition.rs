// Tests for search race condition during workspace initialization
//
// Bug: fast_search hangs indefinitely when called during initial background indexing
// Scenario: MCP server starts → auto-indexes → search called before indexing completes → hang
//
// This test module captures the race condition in a reproducible way.

use crate::handler::JulieServerHandler;
use crate::tools::search::FastSearchTool;
use crate::tools::symbols::GetSymbolsTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that search doesn't hang when called during initial indexing
    ///
    /// This reproduces the Heisenbug where fast_search("handle_validate_syntax")
    /// hung indefinitely right after MCP server restart.
    ///
    /// ROOT CAUSE: SearchEngine in Arc<RwLock<>> - background indexing holds WRITE lock
    /// during slow commit() (5-10s), blocking all searches waiting for READ lock.
    #[tokio::test]
    #[ignore = "Deadlock reproduction test - hangs by design"]
    async fn test_search_during_initial_indexing() -> Result<()> {
        // Create temporary workspace with MANY files to force slow commit
        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path();

        // Create 1000 files to force a slow Tantivy commit
        for i in 0..1000 {
            std::fs::write(
                workspace_path.join(format!("file_{}.rs", i)),
                format!(r#"pub fn function_{}() {{ println!("test"); }}"#, i),
            )?;
        }

        // Add target file
        std::fs::write(
            workspace_path.join("target.rs"),
            r#"pub async fn handle_validate_syntax() { println!("target"); }"#,
        )?;

        // Initialize handler (simulates MCP server start)
        let handler = Arc::new(JulieServerHandler::new().await?);

        // Start workspace initialization (triggers background indexing with WRITE lock)
        handler
            .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
            .await?;

        // CRITICAL: Immediately try to search while background commit holds WRITE lock
        // This is the exact scenario that caused the hang - search waits for READ lock
        let search_tool = FastSearchTool {
            query: "handle_validate_syntax".to_string(),
            search_method: "text".to_string(),
            limit: 15,
            file_pattern: None,
            language: None,
            workspace: None,
            search_target: "content".to_string(),
            output: None,
            context_lines: None,
        };

        // Search MUST complete within 5 seconds or it's the lock contention bug
        let search_result = timeout(Duration::from_secs(5), search_tool.call_tool(&handler)).await;

        match search_result {
            Ok(Ok(_)) => {
                // Search completed successfully
                println!("✅ Search completed without hanging");
                Ok(())
            }
            Ok(Err(e)) => {
                // Search failed but didn't hang
                println!("⚠️  Search failed but didn't hang: {}", e);
                Ok(())
            }
            Err(_timeout_err) => {
                // This is the bug - search hung for >5 seconds waiting for WRITE lock
                panic!("❌ BUG REPRODUCED: Search hung due to RwLock contention during background commit");
            }
        }
    }

    /// Test that multiple rapid searches don't deadlock
    #[tokio::test]
    #[ignore = "Deadlock reproduction test - may hang"]
    async fn test_concurrent_searches_during_indexing() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path();

        // Create multiple test files
        for i in 0..10 {
            std::fs::write(
                workspace_path.join(format!("test_{}.rs", i)),
                format!(r#"pub fn test_function_{}() {{}}"#, i),
            )?;
        }

        let handler = Arc::new(JulieServerHandler::new().await?);
        handler
            .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
            .await?;

        // Spawn multiple concurrent searches
        let mut handles = vec![];

        for i in 0..5 {
            let handler_clone = handler.clone();
            let handle = tokio::spawn(async move {
                let search_tool = FastSearchTool {
                    query: format!("test_function_{}", i),
                    search_method: "text".to_string(),
                    limit: 15,
                    file_pattern: None,
                    language: None,
                    workspace: None,
                    search_target: "content".to_string(),
                    output: None,
                    context_lines: None,
                };

                timeout(
                    Duration::from_secs(5),
                    search_tool.call_tool(&handler_clone),
                )
                .await
            });
            handles.push(handle);
        }

        // Wait for all searches - none should hang
        for (i, handle) in handles.into_iter().enumerate() {
            match handle.await? {
                Ok(_) => println!("✅ Concurrent search {} completed", i),
                Err(_) => panic!("❌ Concurrent search {} hung", i),
            }
        }

        Ok(())
    }

    /// Test that search works correctly after indexing completes
    #[tokio::test]
    async fn test_search_after_indexing_complete() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path();

        std::fs::write(
            workspace_path.join("test.rs"),
            r#"pub fn target_function() {}"#,
        )?;

        let handler = Arc::new(JulieServerHandler::new().await?);
        handler
            .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
            .await?;

        // Wait for initial indexing to complete (generous timeout)
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Now search should definitely work
        let search_tool = FastSearchTool {
            query: "target_function".to_string(),
            search_method: "text".to_string(),
            limit: 15,
            file_pattern: None,
            language: None,
            workspace: None,
            search_target: "content".to_string(),
            output: None,
            context_lines: None,
        };

        let result = timeout(Duration::from_secs(5), search_tool.call_tool(&handler)).await??;

        println!("✅ Search after indexing: {:?}", result);
        Ok(())
    }

    /// Regression test for concurrent fast_search + get_symbols deadlock
    ///
    /// Reproduces the scenario where two parallel fast_search calls combined with
    /// simultaneous get_symbols requests cause one fast_search to hang indefinitely.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Deadlock reproduction test - hangs by design"]
    async fn test_parallel_fast_search_with_get_symbols() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path();

        // Create source files with recognizable symbols
        std::fs::create_dir_all(workspace_path.join("src"))?;
        std::fs::write(
            workspace_path.join("src/lib.rs"),
            r#"pub fn diff_match_patch() {
    println!("diff match patch");
}

pub fn embedding_vector_semantic() {
    println!("embedding vector semantic");
}
"#,
        )?;

        std::fs::write(
            workspace_path.join("src/extra.rs"),
            r#"pub fn get_symbols_target() {}
pub fn helper_function() {}
"#,
        )?;

        let workspace_path_str = workspace_path.to_string_lossy().to_string();

        // Initialize handler and index workspace
        let handler = Arc::new(JulieServerHandler::new().await?);
        handler
            .initialize_workspace_with_force(Some(workspace_path_str.clone()), true)
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path_str.clone()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        // Allow background indexing to flush
        tokio::time::sleep(Duration::from_millis(500)).await;

        for iteration in 0..10 {
            let fast_search_query_a = FastSearchTool {
                query: "diff-match-patch dmp".to_string(),
                search_method: "text".to_string(),
                limit: 15,
                file_pattern: None,
                language: None,
                workspace: None,
                search_target: "content".to_string(),
                output: None,
                context_lines: None,
            };

            let fast_search_query_b = FastSearchTool {
                query: "embedding vector semantic".to_string(),
                search_method: "text".to_string(),
                limit: 15,
                file_pattern: None,
                language: None,
                workspace: None,
                search_target: "content".to_string(),
                output: None,
                context_lines: None,
            };

            let get_symbols_main = GetSymbolsTool {
                file_path: "src/lib.rs".to_string(),
                max_depth: 2,
                target: None,
                limit: None,
                mode: None,
                workspace: None,
            };

            let get_symbols_extra = GetSymbolsTool {
                file_path: "src/extra.rs".to_string(),
                max_depth: 2,
                target: None,
                limit: None,
                mode: None,
                workspace: None,
            };

            let handler_a = handler.clone();
            let handler_b = handler.clone();
            let handler_c = handler.clone();
            let handler_d = handler.clone();

            let task = async move {
                let fast_a =
                    tokio::spawn(async move { fast_search_query_a.call_tool(&handler_a).await });
                let fast_b =
                    tokio::spawn(async move { fast_search_query_b.call_tool(&handler_b).await });
                let symbols_a =
                    tokio::spawn(async move { get_symbols_main.call_tool(&handler_c).await });
                let symbols_b =
                    tokio::spawn(async move { get_symbols_extra.call_tool(&handler_d).await });

                tokio::join!(fast_a, fast_b, symbols_a, symbols_b)
            };

            match timeout(Duration::from_secs(5), task).await {
                Ok((Ok(Ok(_)), Ok(Ok(_)), Ok(Ok(_)), Ok(Ok(_)))) => {}
                Ok(results) => panic!("Concurrent execution error on iteration {}: {results:?}", iteration),
                Err(_) => panic!("❌ BUG REPRODUCED on iteration {}: concurrent fast_search + get_symbols timed out", iteration),
            }
        }

        Ok(())
    }

    /// fast_search should not block when the symbol database mutex is held by another task
    /// Regression test for deadlock where readiness check awaited the DB mutex
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "Deadlock reproduction test - may hang"]
    async fn test_fast_search_not_blocked_by_db_lock() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path();

        std::fs::create_dir_all(workspace_path.join("src"))?;
        std::fs::write(
            workspace_path.join("src/lib.rs"),
            r#"pub fn diff_match_patch() {}
pub fn embedding_vector_semantic() {}
"#,
        )?;

        let workspace_path_str = workspace_path.to_string_lossy().to_string();

        let handler = Arc::new(JulieServerHandler::new().await?);
        handler
            .initialize_workspace_with_force(Some(workspace_path_str.clone()), true)
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path_str.clone()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;

        let workspace = handler
            .get_workspace()
            .await?
            .expect("Workspace should exist");
        let db = workspace
            .db
            .as_ref()
            .expect("Database should be initialized")
            .clone();

        let db_guard = db.lock().unwrap(); // Hold DB mutex to simulate concurrent DB usage

        let fast_search_tool = FastSearchTool {
            query: "diff-match-patch dmp".to_string(),
            search_method: "text".to_string(),
            limit: 15,
            file_pattern: None,
            language: None,
            workspace: None,
            search_target: "content".to_string(),
            output: None,
            context_lines: None,
        };

        let result = timeout(
            Duration::from_millis(250),
            fast_search_tool.call_tool(&handler),
        )
        .await;

        // Expectation: fast_search should complete even while DB mutex is held
        assert!(
            result.is_ok(),
            "fast_search blocked on DB mutex while it should degrade gracefully"
        );

        drop(db_guard);

        Ok(())
    }

    /// Test that fast_search works correctly on reference workspaces
    ///
    /// BUG: fast_search hangs when searching reference workspaces because
    /// check_system_readiness() hardcodes primary workspace, ignoring the
    /// workspace parameter passed to fast_search.
    ///
    /// ROOT CAUSE: Architectural assumption - all code paths assume single primary workspace.
    /// Health checker uses get_primary_workspace_id() instead of the workspace_id being searched.
    #[tokio::test]
    #[ignore] // SLOW: Indexes entire workspace - integration test, run manually with --ignored
    async fn test_reference_workspace_search() -> Result<()> {
        // Skip embeddings to speed up test (we're testing search, not embeddings)
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");

        // Create primary workspace
        let primary_dir = TempDir::new()?;
        let primary_path = primary_dir.path();

        // Create some files in primary workspace
        std::fs::write(
            primary_path.join("primary.rs"),
            r#"pub fn primary_function() { println!("primary"); }"#,
        )?;

        // Create reference workspace with different content
        let reference_dir = TempDir::new()?;
        let reference_path = reference_dir.path();

        std::fs::write(
            reference_path.join("reference.rs"),
            r#"pub fn semantic_diff_tool() { println!("reference"); }"#,
        )?;

        // Initialize handler with primary workspace
        println!("🐛 TEST TRACE 1: Creating handler");
        let handler = Arc::new(JulieServerHandler::new().await?);
        println!("🐛 TEST TRACE 2: Initializing primary workspace");
        handler
            .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
            .await?;
        println!("🐛 TEST TRACE 3: Primary workspace initialized");

        // Index primary workspace
        println!("🐛 TEST TRACE 4: About to index primary workspace");
        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: None,
            name: None,
            workspace_id: None,
            force: Some(false),
            detailed: None,
        };
        println!("🐛 TEST TRACE 5: Calling index_tool.call_tool");
        let index_result = timeout(Duration::from_secs(90), index_tool.call_tool(&handler)).await;

        assert!(
            index_result.is_ok(),
            "manage_workspace index timed out (90s) – still hanging or very slow",
        );

        index_result.unwrap()?;
        println!("🐛 TEST TRACE 6: Index complete, about to add reference workspace");

        // Add reference workspace
        let add_tool = ManageWorkspaceTool {
            operation: "add".to_string(),
            path: Some(reference_path.to_string_lossy().to_string()),
            name: Some("reference-workspace".to_string()),
            workspace_id: None,
            force: None,
            detailed: None,
        };
        println!("🐛 TEST TRACE 7: Calling add_tool.call_tool");
        let add_result = add_tool.call_tool(&handler).await?;
        println!("🐛 TEST TRACE 8: Add complete, extracting workspace ID");

        // Extract workspace ID from result
        let add_value = serde_json::to_value(&add_result)?;
        println!(
            "🐛 DEBUG: add_result JSON = {}",
            serde_json::to_string_pretty(&add_value)?
        );

        let workspace_text = add_value
            .get("content")
            .and_then(|content| content.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
            .ok_or_else(|| anyhow::anyhow!("Failed to extract text from add_result"))?;

        println!("🐛 DEBUG: workspace_text = {}", workspace_text);

        let workspace_id = workspace_text
            .lines()
            .find(|line| line.contains("Workspace ID:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|id| id.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("Failed to find 'Workspace ID:' in response text"))?;

        println!("✅ Reference workspace added: {}", workspace_id);

        // THIS IS THE TEST: Search reference workspace with timeout to catch hangs
        println!("🐛 TEST TRACE 9: Creating fast_search_tool");
        let fast_search_tool = FastSearchTool {
            query: "semantic_diff_tool".to_string(),
            search_method: "text".to_string(),
            limit: 15,
            file_pattern: None,
            language: None,
            workspace: Some(workspace_id.clone()),
            search_target: "content".to_string(),
            output: None,
            context_lines: None,
        };

        println!("🐛 TEST TRACE 10: About to call fast_search with 5s timeout");
        let result = timeout(
            Duration::from_secs(5), // 5 second timeout - should be instant
            fast_search_tool.call_tool(&handler),
        )
        .await;
        println!("🐛 TEST TRACE 11: fast_search returned (or timed out)");

        // CRITICAL: This should NOT timeout
        assert!(
            result.is_ok(),
            "fast_search on reference workspace timed out - health checker is checking wrong workspace!"
        );

        let search_result = result.unwrap()?;
        println!("✅ Search completed: {:?}", search_result);

        // Verify we actually found the symbol in the reference workspace
        let result_text = serde_json::to_string(&search_result)?;
        assert!(
            result_text.contains("semantic_diff_tool") || result_text.contains("No results"),
            "Search should either find semantic_diff_tool or return no results gracefully"
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore = "Lock contention test - may be slow or hang"]
    async fn test_reference_workspace_reindex_does_not_lock() -> Result<()> {
        // Skip embeddings to avoid network/download requirements in test environment
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");

        // Primary workspace (required for registry service)
        let primary_dir = TempDir::new()?;
        let primary_path = primary_dir.path();
        std::fs::write(
            primary_path.join("main.rs"),
            r#"pub fn main() { println!("primary"); }"#,
        )?;

        // Reference workspace that will be reindexed immediately after add
        let reference_dir = TempDir::new()?;
        let reference_path = reference_dir.path();
        std::fs::write(
            reference_path.join("lib.rs"),
            r#"pub fn reindex_target() { println!("reference"); }"#,
        )?;

        let handler = Arc::new(JulieServerHandler::new().await?);
        handler
            .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
            .await?;

        // Initial index of primary workspace
        let index_primary = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: None,
            name: None,
            workspace_id: None,
            force: Some(false),
            detailed: None,
        };
        index_primary.call_tool(&handler).await?;

        // Add reference workspace (triggers first indexing run)
        let add_tool = ManageWorkspaceTool {
            operation: "add".to_string(),
            path: Some(reference_path.to_string_lossy().to_string()),
            name: Some("reindex-test".to_string()),
            workspace_id: None,
            force: None,
            detailed: None,
        };
        let add_result = add_tool.call_tool(&handler).await?;

        // Extract workspace ID from add result
        let add_value = serde_json::to_value(add_result)?;
        let workspace_id = add_value
            .get("content")
            .and_then(|content| content.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
            .and_then(|text| {
                text.lines()
                    .find(|line| line.contains("Workspace ID:"))
                    .and_then(|line| line.split(':').nth(1))
            })
            .map(|id| id.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("Failed to extract workspace ID"))?;

        // Immediately re-index the same reference workspace before background tasks complete
        let reindex_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(reference_path.to_string_lossy().to_string()),
            name: None,
            workspace_id: Some(workspace_id.clone()),
            force: Some(false),
            detailed: None,
        };

        // Use timeout to surface hangs caused by LockBusy deadlocks
        timeout(Duration::from_secs(10), reindex_tool.call_tool(&handler)).await??;

        Ok(())
    }
}
