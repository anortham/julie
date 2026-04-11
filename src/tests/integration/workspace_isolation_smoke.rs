//! Workspace Isolation Smoke Tests
//!
//! Fast, focused tests that verify workspace isolation boundaries.
//! These tests should complete in <500ms each and use fixtures exclusively.

#[cfg(test)]
mod workspace_isolation_smoke_tests {
    use crate::handler::JulieServerHandler;
    use crate::tools::search::FastSearchTool;
    use crate::tools::workspace::ManageWorkspaceTool;
    use anyhow::Result;
    use std::sync::atomic::Ordering;

    fn get_fixture_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/test-workspaces")
            .join(name)
    }

    async fn mark_index_ready(handler: &JulieServerHandler) {
        handler
            .indexing_status
            .search_ready
            .store(true, Ordering::Relaxed);
        *handler.is_indexed.write().await = true;
    }

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

    /// Test 1: Verify search NEVER crosses workspace boundaries
    ///
    /// This is the most critical isolation test - searching primary workspace
    /// should NEVER return results from reference workspace and vice versa.
    #[tokio::test(flavor = "multi_thread")]
    #[serial_test::serial] // Shared fixtures (tiny-primary, tiny-reference)
    async fn test_search_never_crosses_workspaces() -> Result<()> {
        let primary_path = get_fixture_path("tiny-primary");
        let reference_path = get_fixture_path("tiny-reference");

        let handler = JulieServerHandler::new_for_test().await?;
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
                force: Some(false),
                name: None,
                workspace_id: None,
                detailed: None,
            };
            index_reference.call_tool(&handler).await?;
            mark_index_ready(&handler).await;
        }
        let reference_id =
            crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())?;

        // CRITICAL TEST: Search primary for reference-only content
        let search_primary_for_ref = FastSearchTool {
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

        let result = search_primary_for_ref.call_tool(&handler).await?;
        let response = extract_text_from_result(&result);

        // Should find NOTHING - reference content must not leak into primary search
        assert!(
            !response.contains("REFERENCE_WORKSPACE_MARKER") || response.contains("No lines found"),
            "PRIMARY workspace search MUST NOT find reference workspace content!\n\
             Isolation boundary violated: {}",
            response
        );

        // CRITICAL TEST: Search reference for primary-only content
        let search_ref_for_primary = FastSearchTool {
            query: "PRIMARY_WORKSPACE_MARKER".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id),
            search_target: "content".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result2 = search_ref_for_primary.call_tool(&handler).await?;
        let response2 = extract_text_from_result(&result2);

        // Should find NOTHING - primary content must not leak into reference search
        assert!(
            !response2.contains("PRIMARY_WORKSPACE_MARKER") || response2.contains("No lines found"),
            "REFERENCE workspace search MUST NOT find primary workspace content!\n\
             Isolation boundary violated: {}",
            response2
        );

        Ok(())
    }

    /// Test 2: Verify workspace ID resolution works correctly
    ///
    /// Tests that "primary" resolves to primary workspace and specific IDs
    /// resolve to their respective reference workspaces.
    #[tokio::test(flavor = "multi_thread")]
    #[serial_test::serial] // Shared fixtures (tiny-primary, tiny-reference)
    async fn test_workspace_id_resolution() -> Result<()> {
        let primary_path = get_fixture_path("tiny-primary");
        let reference_path = get_fixture_path("tiny-reference");

        let handler = JulieServerHandler::new_for_test().await?;
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
                force: Some(false),
                name: None,
                workspace_id: None,
                detailed: None,
            };
            index_reference.call_tool(&handler).await?;
            mark_index_ready(&handler).await;
        }
        let reference_id =
            crate::workspace::registry::generate_workspace_id(&reference_path.to_string_lossy())?;

        // Test 1: "primary" should resolve to primary workspace
        let search_primary_string = FastSearchTool {
            query: "calculate_sum".to_string(), // Function only in primary
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "definitions".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result1 = search_primary_string.call_tool(&handler).await?;
        let response1 = extract_text_from_result(&result1);

        assert!(
            response1.contains("calculate_sum"),
            "workspace='primary' should resolve to primary workspace: {}",
            response1
        );

        // Test 2: Specific workspace ID should resolve to reference workspace
        let search_ref_id = FastSearchTool {
            query: "calculate_product".to_string(), // Function only in reference
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id),
            search_target: "definitions".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result2 = search_ref_id.call_tool(&handler).await?;
        let response2 = extract_text_from_result(&result2);

        assert!(
            response2.contains("calculate_product"),
            "workspace=<id> should resolve to reference workspace: {}",
            response2
        );

        Ok(())
    }

    /// Test 3: Verify stdio mode treats an unknown workspace ID as an isolated reference target
    ///
    /// In stdio mode there is no workspace registry, so non-`primary` IDs are accepted
    /// permissively. The search should succeed, but it must not fall back to the primary
    /// workspace when that reference workspace does not exist.
    #[tokio::test(flavor = "multi_thread")]
    #[serial_test::serial] // Shared fixtures (tiny-primary, tiny-reference)
    async fn test_invalid_workspace_id_returns_error() -> Result<()> {
        let primary_path = get_fixture_path("tiny-primary");

        let handler = JulieServerHandler::new_for_test().await?;
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

        // Search for a symbol that exists in the primary workspace using an unknown workspace ID.
        // In stdio mode this should be treated as an isolated reference workspace, not an error
        // and not a fallback to primary.
        let search_invalid = FastSearchTool {
            query: "calculate_sum".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("nonexistent_workspace_12345".to_string()),
            search_target: "definitions".to_string(),
            context_lines: None,
            exclude_tests: None,
            ..Default::default()
        };

        let result = search_invalid.call_tool(&handler).await?;
        let response = extract_text_from_result(&result);

        assert!(
            response.contains("Workspace not indexed yet"),
            "Searching with an unknown workspace ID in stdio mode should return the unindexed-workspace message: {}",
            response
        );
        assert!(
            response.contains("manage_workspace(operation=\"index\")"),
            "Search response should preserve the normal indexing guidance: {}",
            response
        );
        assert!(
            !response.contains("src/main.rs"),
            "Unknown workspace ID must not fall back to the primary workspace: {}",
            response
        );

        Ok(())
    }
}
