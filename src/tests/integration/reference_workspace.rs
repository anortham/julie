//! Reference Workspace Tests
//!
//! Tests the complete reference workspace functionality:
//! - Adding reference workspaces
//! - Indexing reference workspaces with isolated databases
//! - Searching reference workspaces by workspace_id
//! - Proper isolation between primary and reference workspaces

#[cfg(test)]
mod reference_workspace_tests {
    use crate::handler::JulieServerHandler;
    use crate::tools::search::FastSearchTool;
    use crate::tools::workspace::ManageWorkspaceTool;
    use anyhow::Result;
    use std::fs;
    use std::sync::atomic::Ordering;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

    /// Extract text from CallToolResult safely
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

    fn extract_workspace_id(result: &rust_mcp_sdk::schema::CallToolResult) -> Option<String> {
        let text = extract_text_from_result(result);
        text.lines()
            .find(|line| line.contains("Workspace ID:"))
            .and_then(|line| line.split(':').nth(1))
            .map(|id| id.trim().to_string())
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_reference_workspace_end_to_end() -> Result<()> {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");

        // Create primary workspace
        let primary_temp = TempDir::new()?;
        let primary_path = primary_temp.path().to_path_buf();
        let primary_src = primary_path.join("src");
        fs::create_dir_all(&primary_src)?;

        // Create file with UNIQUE content in primary workspace
        let primary_file = primary_src.join("primary_marker.rs");
        fs::write(
            &primary_file,
            r#"// This is PRIMARY workspace content
pub fn primary_function() {
    println!("PRIMARY_MARKER_UNIQUE");
}
"#,
        )?;

        // Create reference workspace in DIFFERENT location
        let reference_temp = TempDir::new()?;
        let reference_path = reference_temp.path().to_path_buf();
        let reference_src = reference_path.join("src");
        fs::create_dir_all(&reference_src)?;

        // Create file with DIFFERENT unique content in reference workspace
        let reference_file = reference_src.join("reference_marker.rs");
        fs::write(
            &reference_file,
            r#"// This is REFERENCE workspace content
pub fn reference_function() {
    println!("REFERENCE_MARKER_UNIQUE");
}
"#,
        )?;

        // Initialize handler with primary workspace
        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(primary_path.to_string_lossy().to_string()))
            .await?;

        // Index primary workspace
        let primary_index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(primary_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
            limit: None,
        };
        primary_index_tool.call_tool(&handler).await?;
        sleep(Duration::from_millis(500)).await;
        mark_index_ready(&handler).await;

        // Add reference workspace
        let add_reference_tool = ManageWorkspaceTool {
            operation: "add".to_string(),
            path: Some(reference_path.to_string_lossy().to_string()),
            force: None,
            name: Some("Reference Test Workspace".to_string()),
            workspace_id: None,
            expired_only: None,
            days: None,
            max_size_mb: None,
            detailed: None,
            limit: None,
        };
        let add_result = add_reference_tool.call_tool(&handler).await?;
        let add_response = extract_text_from_result(&add_result);

        println!("Add reference workspace response:\n{}", add_response);

        // Extract workspace ID from response
        let reference_workspace_id = extract_workspace_id(&add_result)
            .ok_or_else(|| anyhow::anyhow!("Failed to extract workspace ID from add response"))?;

        println!("Reference workspace ID: {}", reference_workspace_id);

        // Wait for indexing to complete
        sleep(Duration::from_millis(1000)).await;
        mark_index_ready(&handler).await;

        // Search primary workspace - should find PRIMARY_MARKER_UNIQUE
        let search_primary = FastSearchTool {
            query: "PRIMARY_MARKER_UNIQUE".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            output: Some("lines".to_string()),
        };

        let primary_result = search_primary.call_tool(&handler).await?;
        let primary_response = extract_text_from_result(&primary_result);

        println!("Primary workspace search results:\n{}", primary_response);

        assert!(
            primary_response.contains("PRIMARY_MARKER_UNIQUE"),
            "Primary workspace search should find PRIMARY_MARKER_UNIQUE"
        );
        assert!(
            !primary_response.contains("REFERENCE_MARKER_UNIQUE"),
            "Primary workspace search should NOT find reference workspace content"
        );

        // Search reference workspace by ID - should find REFERENCE_MARKER_UNIQUE
        let search_reference = FastSearchTool {
            query: "REFERENCE_MARKER_UNIQUE".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some(reference_workspace_id.clone()),
            output: Some("lines".to_string()),
        };

        let reference_result = search_reference.call_tool(&handler).await?;
        let reference_response = extract_text_from_result(&reference_result);

        println!("Reference workspace search results:\n{}", reference_response);

        assert!(
            reference_response.contains("REFERENCE_MARKER_UNIQUE"),
            "Reference workspace search should find REFERENCE_MARKER_UNIQUE: {}",
            reference_response
        );
        assert!(
            !reference_response.contains("PRIMARY_MARKER_UNIQUE"),
            "Reference workspace search should NOT find primary workspace content"
        );

        // Verify workspace isolation: search primary for reference content should find nothing
        let cross_search = FastSearchTool {
            query: "REFERENCE_MARKER_UNIQUE".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("primary".to_string()),
            output: Some("lines".to_string()),
        };

        let cross_result = cross_search.call_tool(&handler).await?;
        let cross_response = extract_text_from_result(&cross_result);

        println!("Cross-workspace search results:\n{}", cross_response);

        // Check for actual file content match (not just the marker in the error message)
        // The error message will say "No lines found matching: 'REFERENCE_MARKER_UNIQUE'"
        // but there should be no actual line content with the marker
        assert!(
            cross_response.contains("No lines found") || !cross_response.contains(".rs:"),
            "Primary workspace should NOT contain reference workspace content (isolation verification): {}",
            cross_response
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_invalid_reference_workspace_id_error() -> Result<()> {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "1");

        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
            .await?;

        // Try to search with non-existent workspace ID
        let search_tool = FastSearchTool {
            query: "anything".to_string(),
            mode: "text".to_string(),
            language: None,
            file_pattern: None,
            limit: 10,
            workspace: Some("nonexistent_workspace_12345".to_string()),
            output: Some("lines".to_string()),
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

        Ok(())
    }
}
