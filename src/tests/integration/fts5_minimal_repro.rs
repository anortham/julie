//! Minimal FTS5 corruption reproduction test
//! Goal: Find the exact step that causes FTS5 corruption

#[cfg(test)]
mod fts5_minimal_tests {
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
            .sqlite_fts_ready
            .store(true, Ordering::Relaxed);
        handler
            .indexing_status
            .semantic_ready
            .store(true, Ordering::Relaxed);
        *handler.is_indexed.write().await = true;
    }

    fn extract_text_from_result(result: &rust_mcp_sdk::schema::CallToolResult) -> String {
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
            .collect::<Vec<String>>()
            .join("\n")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_minimal_fts5_corruption_step1_index_only() -> Result<()> {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");

        let primary_path = get_fixture_path("tiny-primary");
        let handler = JulieServerHandler::new().await?;

        // Step 1: Initialize with force=true
        println!("STEP 1: Initialize workspace");
        handler
            .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
            .await?;

        // Step 2: Index primary workspace
        println!("STEP 2: Index primary workspace");
        let index_primary = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(primary_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        };
        let result = index_primary.call_tool(&handler).await?;
        println!("Index result: {}", extract_text_from_result(&result));
        mark_index_ready(&handler).await;

        // Step 3: Immediately search (does FTS5 work right after indexing?)
        println!("STEP 3: Search immediately after indexing");
        let search = FastSearchTool {
            query: "PRIMARY_WORKSPACE_MARKER".to_string(),
            search_method: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            output: Some("lines".to_string()),
            context_lines: None,
        };

        let search_result = search.call_tool(&handler).await?;
        let response = extract_text_from_result(&search_result);
        println!("Search result: {}", response);

        assert!(
            response.contains("PRIMARY_WORKSPACE_MARKER"),
            "Should find marker in freshly indexed workspace: {}",
            response
        );

        println!("✅ TEST PASSED: FTS5 works immediately after indexing");
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_minimal_fts5_corruption_step2_add_reference() -> Result<()> {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");

        let primary_path = get_fixture_path("tiny-primary");
        let reference_path = get_fixture_path("tiny-reference");
        let handler = JulieServerHandler::new().await?;

        // Step 1: Initialize with force=true
        println!("STEP 1: Initialize primary workspace");
        handler
            .initialize_workspace_with_force(Some(primary_path.to_string_lossy().to_string()), true)
            .await?;

        // Step 2: Index primary workspace
        println!("STEP 2: Index primary workspace");
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

        // Step 3: Search primary workspace BEFORE adding reference (should work)
        println!("STEP 3: Search primary BEFORE adding reference");
        let search = FastSearchTool {
            query: "PRIMARY_WORKSPACE_MARKER".to_string(),
            search_method: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            search_target: "content".to_string(),
            output: Some("lines".to_string()),
            context_lines: None,
        };

        let result1 = search.call_tool(&handler).await?;
        let response1 = extract_text_from_result(&result1);
        println!("Search result BEFORE reference: {}", response1);
        assert!(
            response1.contains("PRIMARY_WORKSPACE_MARKER"),
            "Should find marker before adding reference"
        );

        // Step 4: Add reference workspace
        println!("STEP 4: Add reference workspace");
        let add_reference = ManageWorkspaceTool {
            operation: "add".to_string(),
            path: Some(reference_path.to_string_lossy().to_string()),
            force: None,
            name: Some("Reference Test Workspace".to_string()),
            workspace_id: None,
            detailed: None,
        };
        let add_result = add_reference.call_tool(&handler).await?;
        println!(
            "Add reference result: {}",
            extract_text_from_result(&add_result)
        );
        mark_index_ready(&handler).await;

        // Step 5: Search primary workspace AFTER adding reference (does it still work?)
        println!(
            "STEP 5: Search primary AFTER adding reference - THIS IS WHERE CORRUPTION MIGHT HAPPEN"
        );
        let result2 = search.call_tool(&handler).await?;
        let response2 = extract_text_from_result(&result2);
        println!("Search result AFTER reference: {}", response2);

        assert!(
            response2.contains("PRIMARY_WORKSPACE_MARKER"),
            "🔴 BUG: Primary workspace FTS5 corrupted after adding reference workspace!\nResponse: {}",
            response2
        );

        println!("✅ TEST PASSED: FTS5 still works after adding reference workspace");
        Ok(())
    }
}
