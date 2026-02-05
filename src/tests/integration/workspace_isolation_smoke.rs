//! Workspace Isolation Smoke Tests
//!
//! Fast, focused tests that verify workspace isolation boundaries.
//! These tests should complete in <500ms each and use fixtures exclusively.

#[cfg(test)]
mod workspace_isolation_smoke_tests {
    use crate::handler::JulieServerHandler;
    use crate::mcp_compat::StructuredContentExt;
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
        // Try extracting from .content first (TOON mode)
        if !result.content.is_empty() {
            return result
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
                .join("\n");
        }

        // Fall back to .structured_content (JSON mode)
        if let Some(structured) = result.structured_content() {
            return serde_json::to_string_pretty(&structured).unwrap_or_default();
        }

        String::new()
    }

    fn extract_workspace_id(result: &crate::mcp_compat::CallToolResult) -> Option<String> {
        let text = extract_text_from_result(result);
        text.lines()
            .find(|line| line.contains("Workspace ID:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|id| id.trim().to_string())
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

        let handler = JulieServerHandler::new().await?;
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

        // Add and index reference workspace (or get existing if already registered)
        let reference_id = if let Ok(Some(workspace)) = handler.get_workspace().await {
            use crate::workspace::registry_service::WorkspaceRegistryService;
            let registry_service = WorkspaceRegistryService::new(workspace.root.clone());

            // Check if reference workspace already exists (fixture persistence between runs)
            match registry_service
                .get_workspace_by_path(&reference_path.to_string_lossy().to_string())
                .await?
            {
                Some(ws) => {
                    println!("✅ Reference workspace already registered: {}", ws.id);

                    // CRITICAL FIX: Index the existing workspace (it may be registered but not indexed)
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

                    ws.id
                }
                None => {
                    // Add new reference workspace
                    let add_reference = ManageWorkspaceTool {
                        operation: "add".to_string(),
                        path: Some(reference_path.to_string_lossy().to_string()),
                        force: None,
                        name: Some("Reference Smoke Test".to_string()),
                        workspace_id: None,
                        detailed: None,
                    };
                    let reference_result = add_reference.call_tool(&handler).await?;
                    mark_index_ready(&handler).await;

                    extract_workspace_id(&reference_result).ok_or_else(|| {
                        anyhow::anyhow!("Failed to extract reference workspace ID")
                    })?
                }
            }
        } else {
            return Err(anyhow::anyhow!("Failed to get workspace from handler"));
        };

        // CRITICAL TEST: Search primary for reference-only content
        let search_primary_for_ref = FastSearchTool {
            query: "REFERENCE_WORKSPACE_MARKER".to_string(),
            search_method: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            output: Some("lines".to_string()),
            context_lines: None,
        output_format: None,
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
            search_method: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id),
            search_target: "content".to_string(),
            output: Some("lines".to_string()),
            context_lines: None,
        output_format: None,
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

        let handler = JulieServerHandler::new().await?;
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

        // Add reference workspace (or get existing if already registered)
        let reference_id = if let Ok(Some(workspace)) = handler.get_workspace().await {
            use crate::workspace::registry_service::WorkspaceRegistryService;
            let registry_service = WorkspaceRegistryService::new(workspace.root.clone());

            // Check if reference workspace already exists (fixture persistence between runs)
            match registry_service
                .get_workspace_by_path(&reference_path.to_string_lossy().to_string())
                .await?
            {
                Some(ws) => {
                    println!("✅ Reference workspace already registered: {}", ws.id);

                    // CRITICAL FIX: Index the existing workspace (it may be registered but not indexed)
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

                    ws.id
                }
                None => {
                    // Add new reference workspace
                    let add_reference = ManageWorkspaceTool {
                        operation: "add".to_string(),
                        path: Some(reference_path.to_string_lossy().to_string()),
                        force: None,
                        name: Some("Reference Smoke Test".to_string()),
                        workspace_id: None,
                        detailed: None,
                    };
                    let reference_result = add_reference.call_tool(&handler).await?;
                    mark_index_ready(&handler).await;

                    extract_workspace_id(&reference_result).ok_or_else(|| {
                        anyhow::anyhow!("Failed to extract reference workspace ID")
                    })?
                }
            }
        } else {
            return Err(anyhow::anyhow!("Failed to get workspace from handler"));
        };

        // Test 1: "primary" should resolve to primary workspace
        let search_primary_string = FastSearchTool {
            query: "calculate_sum".to_string(), // Function only in primary
            search_method: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "definitions".to_string(),
            output: Some("symbols".to_string()),
            context_lines: None,
        output_format: None,
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
            search_method: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_id),
            search_target: "definitions".to_string(),
            output: Some("symbols".to_string()),
            context_lines: None,
        output_format: None,
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

    /// Test 3: Verify invalid workspace ID returns clear error
    ///
    /// Attempting to search a non-existent workspace should return a
    /// helpful error message, not crash or return confusing results.
    #[tokio::test(flavor = "multi_thread")]
    #[serial_test::serial] // Shared fixtures (tiny-primary, tiny-reference)
    async fn test_invalid_workspace_id_returns_error() -> Result<()> {
        let primary_path = get_fixture_path("tiny-primary");

        let handler = JulieServerHandler::new().await?;
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

        // Try to search non-existent workspace
        let search_invalid = FastSearchTool {
            query: "anything".to_string(),
            search_method: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("nonexistent_workspace_12345".to_string()),
            search_target: "definitions".to_string(),
            output: Some("symbols".to_string()),
            context_lines: None,
        output_format: None,
        };

        let result = search_invalid.call_tool(&handler).await;

        // Should return an error
        assert!(
            result.is_err(),
            "Searching with invalid workspace ID should return error"
        );

        let error_message = result.unwrap_err().to_string();
        assert!(
            error_message.contains("not found") || error_message.contains("does not exist"),
            "Error message should indicate workspace not found: {}",
            error_message
        );

        Ok(())
    }
}
