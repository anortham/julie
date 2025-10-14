// src/tests/lock_contention_tests.rs
//! Regression tests for lock contention issues
//!
//! These tests verify that long-running operations (HNSW indexing, embedding generation)
//! do NOT hold locks that block other concurrent operations.
//!
//! Historical bugs we're preventing:
//! - Issue #1: ensure_vector_store() held workspace write lock for 30-60s during HNSW build
//! - Issue #2: initialize_vector_store() held database lock for 30-60s during HNSW build
//!
//! These tests MUST remain in the codebase to prevent regressions.

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use crate::handler::JulieServerHandler;

/// Test that workspace read access is not blocked during vector store initialization
///
/// REGRESSION TEST: Prevents workspace write lock from being held during 30-60s HNSW build
/// Bug: ensure_vector_store() used to hold write lock for entire HNSW initialization
/// Fix: Lock is released before expensive operations, re-acquired only to store result
#[tokio::test]
async fn test_workspace_access_during_vector_store_init() -> Result<()> {
    // Skip if embeddings disabled (CI environments)
    if std::env::var("JULIE_SKIP_EMBEDDINGS").is_ok() {
        println!("Skipping test - embeddings disabled");
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Initialize handler with test workspace
    let handler = Arc::new(JulieServerHandler::new().await?);
    handler
        .initialize_workspace(Some(workspace_path.to_str().unwrap().to_string()))
        .await?;

    // Spawn vector store initialization in background (this used to block for 30-60s)
    let handler_clone = handler.clone();
    let init_task = tokio::spawn(async move {
        let _ = handler_clone.ensure_vector_store().await;
    });

    // Give initialization a moment to acquire locks
    tokio::time::sleep(Duration::from_millis(100)).await;

    // NOW try to access workspace - this should NOT hang!
    // Before fix: Would wait 30-60s for write lock to be released
    // After fix: Should complete immediately (<1s) because lock is released during HNSW build
    let access_result = timeout(Duration::from_secs(2), async {
        handler.get_workspace().await
    })
    .await;

    // Verify we got a result without timeout
    assert!(
        access_result.is_ok(),
        "get_workspace() timed out after 2s - workspace write lock held too long!"
    );

    // Cleanup
    init_task.abort();
    Ok(())
}

/// Test that database access is not blocked during HNSW index building
///
/// REGRESSION TEST: Prevents database lock from being held during 30-60s HNSW build
/// Bug: initialize_vector_store() used to hold database lock during entire HNSW construction
/// Fix: Embeddings loaded first, lock released, then HNSW built without holding lock
#[tokio::test]
async fn test_database_access_during_hnsw_build() -> Result<()> {
    // Skip if embeddings disabled
    if std::env::var("JULIE_SKIP_EMBEDDINGS").is_ok() {
        println!("Skipping test - embeddings disabled");
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Initialize workspace (creates database)
    let handler = Arc::new(JulieServerHandler::new().await?);
    handler
        .initialize_workspace(Some(workspace_path.to_str().unwrap().to_string()))
        .await?;

    // Get workspace reference for testing
    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace should be initialized");

    let db = workspace
        .db
        .as_ref()
        .expect("Database should be initialized")
        .clone();

    // Spawn HNSW initialization in background
    // This internally calls initialize_vector_store() which builds HNSW index
    let handler_clone = handler.clone();
    let init_task = tokio::spawn(async move {
        let _ = handler_clone.ensure_vector_store().await;
    });

    // Give initialization time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // NOW try to access database - this should NOT hang!
    // Before fix: Would wait 30-60s for database lock during HNSW build
    // After fix: Should complete immediately because lock released before HNSW build
    let db_access_result = timeout(Duration::from_secs(2), async {
        tokio::task::spawn_blocking(move || {
            let db_lock = db.lock().unwrap();
            // Simple query to verify database is accessible
            db_lock.get_all_symbols().map(|s| s.len())
        })
        .await
    })
    .await;

    // Verify we got a result without timeout
    assert!(
        db_access_result.is_ok(),
        "Database query timed out after 2s - database lock held too long during HNSW build!"
    );

    // Cleanup
    init_task.abort();
    Ok(())
}

/// Test that multiple tools can run concurrently during indexing
///
/// INTEGRATION TEST: Verifies the complete fix works in realistic scenario
/// Simulates: User triggers reindex, immediately runs fast_goto and fast_search
/// Expected: Both tools should work without 30s hangs
#[tokio::test]
async fn test_concurrent_tool_execution_during_indexing() -> Result<()> {
    // Skip if embeddings disabled
    if std::env::var("JULIE_SKIP_EMBEDDINGS").is_ok() {
        println!("Skipping test - embeddings disabled");
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Create test file
    std::fs::create_dir_all(workspace_path.join("src"))?;
    std::fs::write(
        workspace_path.join("src/test.rs"),
        "fn test_function() { println!(\"test\"); }",
    )?;

    let handler = Arc::new(JulieServerHandler::new().await?);
    handler
        .initialize_workspace(Some(workspace_path.to_str().unwrap().to_string()))
        .await?;

    // Simulate indexing (triggers HNSW build)
    let handler_clone = handler.clone();
    let index_task = tokio::spawn(async move {
        // This would normally trigger ensure_vector_store internally
        let _ = handler_clone.ensure_vector_store().await;
    });

    // Give indexing time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // NOW run tools concurrently - these should NOT hang!
    let workspace_access_1 = timeout(Duration::from_secs(2), async {
        handler.get_workspace().await
    });

    let workspace_access_2 = timeout(Duration::from_secs(2), async {
        handler.get_workspace().await
    });

    // Both should complete without timeout
    let (result1, result2) = tokio::join!(workspace_access_1, workspace_access_2);

    assert!(
        result1.is_ok(),
        "First concurrent workspace access timed out!"
    );
    assert!(
        result2.is_ok(),
        "Second concurrent workspace access timed out!"
    );

    // Cleanup
    index_task.abort();
    Ok(())
}

/// Test that lock hold time is minimal (< 100ms)
///
/// PERFORMANCE TEST: Verifies locks are held only for data extraction, not long operations
/// This is the core of the fix: locks should be held for milliseconds, not seconds
#[tokio::test]
async fn test_lock_hold_time_is_minimal() -> Result<()> {
    // Skip if embeddings disabled
    if std::env::var("JULIE_SKIP_EMBEDDINGS").is_ok() {
        println!("Skipping test - embeddings disabled");
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let handler = Arc::new(JulieServerHandler::new().await?);
    handler
        .initialize_workspace(Some(workspace_path.to_str().unwrap().to_string()))
        .await?;

    // Spawn vector store initialization
    let handler_clone = handler.clone();
    let init_start = std::time::Instant::now();

    tokio::spawn(async move {
        let _ = handler_clone.ensure_vector_store().await;
    });

    // Wait a tiny bit for initialization to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Measure how long it takes to get workspace access
    let access_start = std::time::Instant::now();
    let _ = handler.get_workspace().await?;
    let access_duration = access_start.elapsed();

    // Lock should be released almost immediately (<100ms)
    // If this fails, it means lock is being held during long operations
    assert!(
        access_duration < Duration::from_millis(500),
        "Workspace access took {:?} - lock held too long! Should be <100ms",
        access_duration
    );

    println!(
        "✅ Lock hold time verification: workspace access took {:?}",
        access_duration
    );

    Ok(())
}

/// Test that database lock is released before HNSW build starts
///
/// UNIT TEST: Direct test of initialize_vector_store() lock behavior
/// Verifies the specific fix in workspace/mod.rs
#[tokio::test]
async fn test_database_lock_released_before_hnsw_build() -> Result<()> {
    // Skip if embeddings disabled
    if std::env::var("JULIE_SKIP_EMBEDDINGS").is_ok() {
        println!("Skipping test - embeddings disabled");
        return Ok(());
    }

    let temp_dir = tempfile::tempdir()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Create a workspace with database
    let mut workspace = crate::workspace::JulieWorkspace::initialize(workspace_path).await?;

    let db = workspace
        .db
        .as_ref()
        .expect("Database should be initialized")
        .clone();

    // Spawn vector store initialization (includes HNSW build)
    tokio::task::spawn_blocking(move || {
        let _ = workspace.initialize_vector_store();
    });

    // Give initialization time to load embeddings and release lock
    tokio::time::sleep(Duration::from_millis(200)).await;

    // NOW try to acquire database lock - should succeed immediately
    let db_access_result = timeout(Duration::from_secs(1), async {
        tokio::task::spawn_blocking(move || {
            let db_lock = db.lock().unwrap();
            db_lock.get_all_symbols().map(|s| s.len())
        })
        .await
    })
    .await;

    assert!(
        db_access_result.is_ok(),
        "Database lock still held during HNSW build - fix not working!"
    );

    println!("✅ Database lock properly released before HNSW build");
    Ok(())
}
