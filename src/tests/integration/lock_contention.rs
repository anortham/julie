// src/tests/lock_contention_tests.rs
//! Regression tests for lock contention issues
//!
//! These tests verify that concurrent database operations do NOT cause
//! corruption or deadlocks.
//!
//! Historical bugs we're preventing:
//! - Concurrent content searches causing "database disk image is malformed" errors

#![allow(unused_variables)]

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use crate::handler::JulieServerHandler;

/// Test that concurrent content searches don't cause database corruption errors
///
/// REGRESSION TEST: Prevents "database disk image is malformed" errors from concurrent access
/// Bug: sqlite_fts_search() called db.lock() outside of block_in_place in async context
/// Fix: All database access must be wrapped in block_in_place to prevent Tokio race conditions
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_content_searches_no_corruption() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Create test files with searchable content (matching the original bug scenario)
    std::fs::create_dir_all(workspace_path.join("docs/archive"))?;
    std::fs::write(
        workspace_path.join("docs/archive/TOOL_AUDIT.md"),
        "# Tool Audit\n\n## Summary of Findings\n\nMinor Improvements Identified\nOptional Enhancements recommended\n",
    )?;
    std::fs::write(
        workspace_path.join("docs/archive/file2.md"),
        "# Another Document\n\nMore summary findings and recommendations here.\n",
    )?;

    // Initialize handler and index workspace
    let handler = Arc::new(JulieServerHandler::new().await?);
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_str().unwrap().to_string()), true)
        .await?;

    // Spawn 10 concurrent LINE MODE searches (this triggers the bug)
    // Original failure: Line mode searches with file_pattern cause "database disk image is malformed"
    let mut tasks = vec![];
    for i in 0..10 {
        let handler_clone = handler.clone();
        let query = if i % 2 == 0 { "summary findings" } else { "recommendations" };

        let task = tokio::spawn(async move {
            // Use LINE MODE with file pattern - this is what triggers the bug!
            use crate::tools::search::FastSearchTool;
            let tool = FastSearchTool {
                query: query.to_string(),
                language: None,
                file_pattern: Some("docs/archive/*.md".to_string()), // File pattern filter
                limit: 10,
                workspace: Some("primary".to_string()),
                search_target: "content".to_string(),
                context_lines: Some(1),
            };
            tool.call_tool(&handler_clone).await
        });
        tasks.push(task);
    }

    // All tasks should complete without errors (no "database disk image is malformed")
    let results = timeout(Duration::from_secs(5), async {
        let mut all_results = vec![];
        for task in tasks {
            match task.await {
                Ok(result) => all_results.push(result),
                Err(e) => panic!("Task panicked: {}", e),
            }
        }
        all_results
    })
    .await;

    assert!(
        results.is_ok(),
        "Concurrent content searches timed out - possible deadlock!"
    );

    let search_results = results.unwrap();

    // Verify all searches succeeded (no corruption errors)
    for (i, result) in search_results.iter().enumerate() {
        assert!(
            result.is_ok(),
            "Search {} failed with error: {:?}",
            i,
            result.as_ref().err()
        );
    }

    println!("✅ 10 concurrent content searches completed without corruption errors");
    Ok(())
}
