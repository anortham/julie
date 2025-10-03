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
        handler.initialize_workspace(Some(workspace_path.to_string_lossy().to_string())).await?;

        // CRITICAL: Immediately try to search while background commit holds WRITE lock
        // This is the exact scenario that caused the hang - search waits for READ lock
        let search_tool = FastSearchTool {
            query: "handle_validate_syntax".to_string(),
            mode: "text".to_string(),
            limit: 15,
            file_pattern: None,
            language: None,
            workspace: None,
        };

        // Search MUST complete within 5 seconds or it's the lock contention bug
        let search_result = timeout(
            Duration::from_secs(5),
            search_tool.call_tool(&handler)
        ).await;

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
        handler.initialize_workspace(Some(workspace_path.to_string_lossy().to_string())).await?;

        // Spawn multiple concurrent searches
        let mut handles = vec![];

        for i in 0..5 {
            let handler_clone = handler.clone();
            let handle = tokio::spawn(async move {
                let search_tool = FastSearchTool {
                    query: format!("test_function_{}", i),
                    mode: "text".to_string(),
                    limit: 15,
                    file_pattern: None,
                    language: None,
                    workspace: None,
                };

                timeout(
                    Duration::from_secs(5),
                    search_tool.call_tool(&handler_clone)
                ).await
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
        handler.initialize_workspace(Some(workspace_path.to_string_lossy().to_string())).await?;

        // Wait for initial indexing to complete (generous timeout)
        tokio::time::sleep(Duration::from_secs(10)).await;

        // Now search should definitely work
        let search_tool = FastSearchTool {
            query: "target_function".to_string(),
            mode: "text".to_string(),
            limit: 15,
            file_pattern: None,
            language: None,
            workspace: None,
        };

        let result = timeout(
            Duration::from_secs(5),
            search_tool.call_tool(&handler)
        ).await??;

        println!("✅ Search after indexing: {:?}", result);
        Ok(())
    }

    /// Regression test for concurrent fast_search + get_symbols deadlock
    ///
    /// Reproduces the scenario where two parallel fast_search calls combined with
    /// simultaneous get_symbols requests cause one fast_search to hang indefinitely.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
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
            .initialize_workspace(Some(workspace_path_str.clone()))
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path_str.clone()),
            force: Some(false),
            name: None,
            workspace_id: None,
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        // Allow background indexing to flush
        tokio::time::sleep(Duration::from_millis(500)).await;

        for iteration in 0..10 {
            let fast_search_query_a = FastSearchTool {
                query: "diff-match-patch dmp".to_string(),
                mode: "text".to_string(),
                limit: 15,
                file_pattern: None,
                language: None,
                workspace: None,
            };

            let fast_search_query_b = FastSearchTool {
                query: "embedding vector semantic".to_string(),
                mode: "text".to_string(),
                limit: 15,
                file_pattern: None,
                language: None,
                workspace: None,
            };

            let get_symbols_main = GetSymbolsTool {
                file_path: "src/lib.rs".to_string(),
                max_depth: 2,
                include_body: false,
                target: None,
                mode: None,
            };

            let get_symbols_extra = GetSymbolsTool {
                file_path: "src/extra.rs".to_string(),
                max_depth: 2,
                include_body: false,
                target: None,
                mode: None,
            };

            let handler_a = handler.clone();
            let handler_b = handler.clone();
            let handler_c = handler.clone();
            let handler_d = handler.clone();

            let task = async move {
                let fast_a = tokio::spawn(async move { fast_search_query_a.call_tool(&handler_a).await });
                let fast_b = tokio::spawn(async move { fast_search_query_b.call_tool(&handler_b).await });
                let symbols_a = tokio::spawn(async move { get_symbols_main.call_tool(&handler_c).await });
                let symbols_b = tokio::spawn(async move { get_symbols_extra.call_tool(&handler_d).await });

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
            .initialize_workspace(Some(workspace_path_str.clone()))
            .await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path_str.clone()),
            force: Some(false),
            name: None,
            workspace_id: None,
            expired_only: None,
            days: None,
            max_size_mb: None,
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

        let db_guard = db.lock().await; // Hold DB mutex to simulate concurrent DB usage

        let fast_search_tool = FastSearchTool {
            query: "diff-match-patch dmp".to_string(),
            mode: "text".to_string(),
            limit: 15,
            file_pattern: None,
            language: None,
            workspace: None,
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
}
