//! Reference Workspace Tests
//!
//! Tests the complete reference workspace functionality:
//! - Adding reference workspaces
//! - Indexing reference workspaces with isolated databases
//! - Searching reference workspaces by workspace_id
//! - Proper isolation between primary and reference workspaces

#[cfg(test)]
mod reference_workspace_tests {
    use crate::database::ProjectionStatus;
    use crate::handler::JulieServerHandler;
    use crate::search::projection::TANTIVY_PROJECTION_NAME;
    use crate::tests::helpers::workspace::get_fixture_path;
    use crate::tools::search::FastSearchTool;
    use crate::tools::workspace::ManageWorkspaceTool;
    use anyhow::Result;
    use std::sync::atomic::Ordering;

    fn extract_text_from_result(result: &crate::mcp_compat::CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|content_block| {
                serde_json::to_value(content_block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn mark_index_ready(handler: &JulieServerHandler) {
        handler
            .indexing_status
            .search_ready
            .store(true, Ordering::Relaxed);
        *handler.is_indexed.write().await = true;
    }

    /// Setup test workspaces using fixtures
    /// Returns (primary_workspace_id, reference_workspace_id)
    async fn setup_test_workspaces(handler: &JulieServerHandler) -> Result<(String, String)> {
        let primary_path = get_fixture_path("tiny-primary");
        let reference_path = get_fixture_path("tiny-reference");

        // Index primary workspace
        let index_primary = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(primary_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_primary.call_tool(handler).await?;
        mark_index_ready(handler).await;

        // Compute primary workspace ID deterministically from path
        let primary_id = if let Ok(Some(workspace)) = handler.get_workspace().await {
            crate::workspace::registry::generate_workspace_id(&workspace.root.to_string_lossy())
                .map_err(|e| anyhow::anyhow!("Failed to compute primary workspace ID: {}", e))?
        } else {
            return Err(anyhow::anyhow!("Failed to get workspace from handler"));
        };

        // Index reference workspace and compute its ID deterministically
        {
            let index_reference = ManageWorkspaceTool {
                operation: "index".to_string(),
                path: Some(reference_path.to_string_lossy().to_string()),
                force: Some(true),
                name: None,
                workspace_id: None,
                detailed: None,
            };
            index_reference.call_tool(handler).await?;
            mark_index_ready(handler).await;
        }
        let reference_id =
            crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())
                .map_err(|e| anyhow::anyhow!("Failed to compute reference workspace ID: {}", e))?;

        Ok((primary_id, reference_id))
    }

    /// ✅ FIXED: FTS5 CORRUPTION BUG - Bug was in filter_changed_files and clean_orphaned_files
    /// Root cause: When indexing REFERENCE workspace, these functions were querying the PRIMARY
    /// workspace database and deleting all primary files as "orphaned", corrupting FTS5 index.
    ///
    /// Fix: Both functions now check workspace_id and query the correct database:
    /// - Primary workspace: use handler.get_workspace().db
    /// - Reference workspace: open separate DB at indexes/{workspace_id}/db/symbols.db
    ///
    /// REFACTORING STATUS: Complete - uses fixture setup, bug fixed, test passing
    #[tokio::test(flavor = "multi_thread")]
    #[serial_test::serial] // Reference workspace tests need serialization (shared fixtures)
    async fn test_reference_workspace_end_to_end() -> Result<()> {
        use crate::tests::helpers::cleanup::atomic_cleanup_julie_dir;

        // CLEANUP: Atomic cleanup of .julie directories from previous test runs
        let primary_path = get_fixture_path("tiny-primary");
        let reference_path = get_fixture_path("tiny-reference");
        atomic_cleanup_julie_dir(&primary_path)?;
        atomic_cleanup_julie_dir(&reference_path)?;

        // Initialize handler with primary fixture
        let handler = JulieServerHandler::new_for_test().await?;
        // CRITICAL: Use force=true to prevent detect_and_load from walking up to parent .julie/
        handler
            .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
            .await?;

        // Setup both workspaces using fixtures
        let (primary_id, reference_id) = setup_test_workspaces(&handler).await?;

        // Search primary workspace - should find PRIMARY_WORKSPACE_MARKER
        let search_primary = FastSearchTool {
            query: "PRIMARY_WORKSPACE_MARKER".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(primary_id.clone()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let primary_result = search_primary.call_tool(&handler).await?;
        let primary_response = extract_text_from_result(&primary_result);

        assert!(
            primary_response.contains("PRIMARY_WORKSPACE_MARKER"),
            "Primary workspace search should find PRIMARY_WORKSPACE_MARKER"
        );
        assert!(
            !primary_response.contains("REFERENCE_WORKSPACE_MARKER"),
            "Primary workspace search should NOT find reference workspace content"
        );

        // Search reference workspace by ID - should find REFERENCE_WORKSPACE_MARKER
        let search_reference = FastSearchTool {
            query: "REFERENCE_WORKSPACE_MARKER".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id.clone()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let reference_result = search_reference.call_tool(&handler).await?;
        let reference_response = extract_text_from_result(&reference_result);

        assert!(
            reference_response.contains("REFERENCE_WORKSPACE_MARKER"),
            "Reference workspace search should find REFERENCE_WORKSPACE_MARKER: {}",
            reference_response
        );
        assert!(
            !reference_response.contains("PRIMARY_WORKSPACE_MARKER"),
            "Reference workspace search should NOT find primary workspace content"
        );

        // Verify workspace isolation: search primary for reference content should find nothing
        let cross_search = FastSearchTool {
            query: "REFERENCE_WORKSPACE_MARKER".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let cross_result = cross_search.call_tool(&handler).await?;
        let cross_response = extract_text_from_result(&cross_result);

        // Check for actual file content match (not just the marker in the error message)
        // The error message will say "No lines found matching: 'REFERENCE_WORKSPACE_MARKER'"
        // but there should be no actual line content with the marker
        assert!(
            cross_response.contains("No lines found") || !cross_response.contains(".rs:"),
            "Primary workspace should NOT contain reference workspace content (isolation verification): {}",
            cross_response
        );

        // NOTE: No cleanup needed at end - next test cleans up at beginning
        // This avoids Windows file locking issues (OS error 32)

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[serial_test::serial] // Reference workspace tests need serialization (shared fixtures)
    async fn test_invalid_reference_workspace_id_error() -> Result<()> {
        use crate::tests::helpers::cleanup::atomic_cleanup_julie_dir;

        unsafe {
            std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");
        }

        // Use fixture path and cleanup BEFORE test
        let workspace_path = get_fixture_path("tiny-primary");
        atomic_cleanup_julie_dir(&workspace_path)?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        // Try to search with non-existent workspace ID
        let search_tool = FastSearchTool {
            query: "anything".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("nonexistent_workspace_12345".to_string()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_tool.call_tool(&handler).await;

        assert!(
            result.is_err(),
            "Searching with invalid workspace ID should return error"
        );

        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("not found"),
            "Error message should indicate workspace not found: {}",
            error_message
        );

        // NOTE: No cleanup needed at end - next test cleans up at beginning
        // This avoids Windows file locking issues (OS error 32)

        Ok(())
    }

    /// Test that workspace filtering works correctly for searches
    /// Refactored from semantic search test to use text search (faster, no embeddings needed)
    ///
    /// NOTE: This test has isolation issues when run with other reference_workspace tests
    /// (works fine when run alone, fails when run after test_reference_workspace_end_to_end).
    /// The main functionality is already covered by test_reference_workspace_end_to_end.
    /// TODO: Fix test isolation by using completely separate fixture directories per test.
    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "Test isolation issue - passes alone, fails when run with other tests"]
    async fn test_workspace_filtering() -> Result<()> {
        // Initialize handler with primary fixture
        let primary_path = get_fixture_path("tiny-primary");
        let reference_path = get_fixture_path("tiny-reference");

        let handler = JulieServerHandler::new_for_test().await?;
        // CRITICAL: Use force=true to prevent detect_and_load from walking up to parent .julie/
        handler
            .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
            .await?;

        // Index primary workspace
        let index_primary = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(primary_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        index_primary.call_tool(&handler).await?;
        mark_index_ready(&handler).await;

        // Index reference workspace and compute its ID deterministically
        {
            let index_reference = ManageWorkspaceTool {
                operation: "index".to_string(),
                path: Some(reference_path.to_string_lossy().to_string()),
                force: None,
                name: None,
                workspace_id: None,
                detailed: None,
            };
            index_reference.call_tool(&handler).await?;
            mark_index_ready(&handler).await;
        }
        let reference_id =
            crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())
                .map_err(|e| anyhow::anyhow!("Failed to compute reference workspace ID: {}", e))?;

        // Search reference workspace for reference-specific symbol
        let search_reference = FastSearchTool {
            query: "calculate_product".to_string(), // Function only in reference workspace
            language: Some("rust".to_string()),
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id.clone()),
            search_target: "definitions".to_string(), // Use symbols scope (doesn't need FTS5)
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let reference_result = search_reference.call_tool(&handler).await?;
        let reference_response = extract_text_from_result(&reference_result);

        println!(
            "Reference workspace search results:\n{}",
            reference_response
        );

        // Should find reference workspace function
        assert!(
            reference_response.contains("calculate_product"),
            "Reference workspace search should find calculate_product function.\n\
             Instead got: {}",
            reference_response
        );

        // Should NOT find primary workspace functions
        assert!(
            !reference_response.contains("calculate_sum"),
            "Reference workspace search should NOT find primary workspace functions.\n\
             Got: {}",
            reference_response
        );

        Ok(())
    }

    /// Test that orphaned files are cleaned up from reference workspace database
    ///
    /// This verifies the fix for INCOMPLETE_IMPLEMENTATIONS.md Issue #2:
    /// Reference workspace orphan cleanup must open the correct database to clean up deleted files.
    #[tokio::test(flavor = "multi_thread")]
    #[serial_test::serial] // Reference workspace tests need serialization (shared fixtures)
    async fn test_reference_workspace_orphan_cleanup() -> Result<()> {
        // CLEANUP: Remove any stale .julie directories from previous test runs to prevent FTS5 corruption
        let primary_path = get_fixture_path("tiny-primary");
        let reference_path = get_fixture_path("tiny-reference");
        let _ = std::fs::remove_dir_all(primary_path.join(".julie"));
        let _ = std::fs::remove_dir_all(reference_path.join(".julie"));

        // Initialize handler with primary fixture
        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
            .await?;

        // Setup both workspaces using fixtures
        let (_primary_id, reference_id) = setup_test_workspaces(&handler).await?;

        // Verify initial files are indexed in reference workspace
        let initial_search = FastSearchTool {
            query: "helper".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id.clone()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let initial_result = initial_search.call_tool(&handler).await?;
        let initial_response = extract_text_from_result(&initial_result);

        assert!(
            initial_response.contains("helper.rs"),
            "Initial search should find helper.rs: {}",
            initial_response
        );

        let reference_db = handler.get_database_for_workspace(&reference_id).await?;
        let (canonical_before_cleanup, projection_before_cleanup) = {
            let db = reference_db.lock().unwrap();
            (
                db.get_current_canonical_revision(&reference_id)?,
                db.get_projection_state(TANTIVY_PROJECTION_NAME, &reference_id)?,
            )
        };

        // Now simulate file deletion: Delete helper.rs from the reference workspace fixture
        let reference_path = get_fixture_path("tiny-reference");
        let helper_file_path = reference_path.join("src").join("helper.rs");

        // Create a backup and delete the file
        let backup_content = std::fs::read_to_string(&helper_file_path)?;
        std::fs::remove_file(&helper_file_path)?;

        // Re-index the reference workspace to trigger orphan cleanup
        let reindex_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(reference_path.to_string_lossy().to_string()),
            force: Some(false), // Incremental mode should trigger orphan cleanup
            name: None,
            workspace_id: None,
            detailed: None,
        };

        let reindex_result = reindex_tool.call_tool(&handler).await?;
        let reindex_response = extract_text_from_result(&reindex_result);

        assert!(
            !reindex_response.contains("Error") && !reindex_response.contains("Failed"),
            "Reference workspace reindex should succeed before cleanup assertion: {}",
            reindex_response
        );

        // Search for the deleted file - should NOT be found
        let search_deleted = FastSearchTool {
            query: "helper".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id.clone()),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let deleted_result = search_deleted.call_tool(&handler).await?;
        let deleted_response = extract_text_from_result(&deleted_result);

        // Restore the file for other tests
        std::fs::write(&helper_file_path, backup_content)?;

        // Verify orphaned file was cleaned up from database
        assert!(
            !deleted_response.contains("helper.rs") || deleted_response.contains("No lines found"),
            "Orphaned file helper.rs should have been cleaned up from reference workspace database, but was still found: {}",
            deleted_response
        );

        let (canonical_after_cleanup, projection_after_cleanup) = {
            let db = reference_db.lock().unwrap();
            (
                db.get_current_canonical_revision(&reference_id)?,
                db.get_projection_state(TANTIVY_PROJECTION_NAME, &reference_id)?,
            )
        };

        assert!(
            canonical_before_cleanup.is_some(),
            "reference workspace should have a canonical revision before orphan cleanup"
        );
        assert!(
            canonical_after_cleanup > canonical_before_cleanup,
            "orphan cleanup should advance the canonical revision"
        );
        let projection_after_cleanup = projection_after_cleanup
            .expect("reference workspace should keep Tantivy projection state");
        assert_eq!(
            projection_after_cleanup.status,
            ProjectionStatus::Ready,
            "projection state should return to ready after orphan cleanup"
        );
        assert_eq!(
            projection_after_cleanup.canonical_revision, canonical_after_cleanup,
            "projection state should track the new orphan-cleanup revision"
        );
        assert_eq!(
            projection_after_cleanup.projected_revision, canonical_after_cleanup,
            "projection state should be marked current after Tantivy cleanup commits"
        );
        assert!(
            projection_before_cleanup
                .as_ref()
                .is_some_and(|state| state.status == ProjectionStatus::Ready),
            "reference workspace should start from a ready projection state"
        );

        Ok(())
    }
}
