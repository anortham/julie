//! Tests for fast_explore data return bug (TDD: RED phase)
//! Bug: fast_explore returns success: true but provides NO actual exploration data
//!
//! This test file demonstrates the bug where fast_explore returns empty results
//! even though the database contains symbols and relationships.

#[cfg(test)]
mod fast_explore_data_return_tests {
    use crate::handler::JulieServerHandler;
    use crate::tools::exploration::FastExploreTool;
    use crate::tools::ManageWorkspaceTool;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a test workspace with Rust source file
    async fn create_test_workspace_with_data() -> Result<(JulieServerHandler, TempDir), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;

        // Create a Rust file with multiple symbols
        let test_file = src_dir.join("lib.rs");
        fs::write(
            &test_file,
            r#"
pub struct UserService {
    pub name: String,
}

impl UserService {
    pub fn get_user_data(&self) -> String {
        self.validate_email();
        "user data".to_string()
    }

    fn validate_email(&self) -> bool {
        true
    }
}

pub fn fetch_user(id: u32) -> String {
    let service = UserService { name: "test".to_string() };
    service.get_user_data()
}
"#,
        )?;

        // Create handler and initialize it with the workspace
        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
            .await?;

        // Trigger indexing
        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: None,
            force: Some(true),
            name: None,
            detailed: None,
            workspace_id: None,
            days: None,
            expired_only: None,
            max_size_mb: None,
            limit: None,
        };
        index_tool.call_tool(&handler).await?;

        // Wait for indexing to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        Ok((handler, temp_dir))
    }

    /// Test that overview mode returns actual symbol statistics in markdown
    #[tokio::test]
    async fn test_fast_explore_overview_returns_symbol_counts() {
        let (handler, _temp_dir) = create_test_workspace_with_data()
            .await
            .expect("Failed to create test workspace");

        let tool = FastExploreTool {
            mode: "overview".to_string(),
            depth: "medium".to_string(),
            focus: None,
        };

        let result = tool.call_tool(&handler).await.expect("Tool call failed");

        // Get structured content (should have tool metadata)
        let structured = result.structured_content
            .as_ref()
            .expect("Structured content is None");

        // Should have required fields
        assert_eq!(structured.get("tool").and_then(|v| v.as_str()), Some("fast_explore"));
        assert_eq!(structured.get("mode").and_then(|v| v.as_str()), Some("overview"));

        // Should indicate success
        let success = structured.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        assert!(success, "Tool returned success: false");

        // Get the markdown output to verify it contains actual data
        // Note: The TextContent is in result.content
        let content_str = format!("{:?}", result.content);
        println!("Content: {}", content_str);

        // Should mention symbols, files, and languages
        assert!(
            content_str.contains("symbol") && content_str.contains("file"),
            "Overview should report symbols and files. Got: {}",
            content_str
        );
    }

    /// Test that dependencies mode returns actual relationship statistics
    #[tokio::test]
    async fn test_fast_explore_dependencies_returns_relationship_data() {
        let (handler, _temp_dir) = create_test_workspace_with_data()
            .await
            .expect("Failed to create test workspace");

        let tool = FastExploreTool {
            mode: "dependencies".to_string(),
            depth: "medium".to_string(),
            focus: None,
        };

        let result = tool.call_tool(&handler).await.expect("Tool call failed");

        // Should have structured content
        let structured = result.structured_content
            .as_ref()
            .expect("Dependencies mode returns no structured content");

        // Should have required fields
        assert_eq!(structured.get("tool").and_then(|v| v.as_str()), Some("fast_explore"));
        assert_eq!(structured.get("mode").and_then(|v| v.as_str()), Some("dependencies"));

        // Should indicate success
        let success = structured.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        assert!(success, "Dependencies mode returned success: false");

        // Get the markdown output to verify it contains relationship data
        let content_str = format!("{:?}", result.content);
        println!("Dependencies content: {}", content_str);

        // Should mention relationships
        assert!(
            content_str.contains("relationship") || content_str.contains("total"),
            "Dependencies should report relationship counts. Got: {}",
            content_str
        );
    }

    /// Test that hotspots mode returns actual file complexity metrics
    #[tokio::test]
    async fn test_fast_explore_hotspots_returns_file_metrics() {
        let (handler, _temp_dir) = create_test_workspace_with_data()
            .await
            .expect("Failed to create test workspace");

        let tool = FastExploreTool {
            mode: "hotspots".to_string(),
            depth: "medium".to_string(),
            focus: None,
        };

        let result = tool.call_tool(&handler).await.expect("Tool call failed");

        // Should have structured content
        let structured = result.structured_content
            .as_ref()
            .expect("Hotspots mode returns no structured content");

        // Should have required fields
        assert_eq!(structured.get("tool").and_then(|v| v.as_str()), Some("fast_explore"));
        assert_eq!(structured.get("mode").and_then(|v| v.as_str()), Some("hotspots"));

        // Should indicate success
        let success = structured.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        assert!(success, "Hotspots mode returned success: false");

        // Get the markdown output to verify it contains hotspot data
        let content_str = format!("{:?}", result.content);
        println!("Hotspots content: {}", content_str);

        // Should mention files or complexity
        assert!(
            content_str.contains("file") || content_str.contains("complexity") || content_str.contains("analyz"),
            "Hotspots should report file metrics. Got: {}",
            content_str
        );
    }

    /// Test that focus filtering works in overview mode
    #[tokio::test]
    async fn test_fast_explore_overview_with_focus_filter() {
        let (handler, _temp_dir) = create_test_workspace_with_data()
            .await
            .expect("Failed to create test workspace");

        let tool = FastExploreTool {
            mode: "overview".to_string(),
            depth: "medium".to_string(),
            focus: Some("src".to_string()),
        };

        let result = tool.call_tool(&handler).await.expect("Tool call failed");

        // Get structured content
        let structured = result.structured_content
            .as_ref()
            .expect("Focus filter test: no structured content");

        // Should have focus field set
        assert_eq!(
            structured.get("focus").and_then(|v| v.as_str()),
            Some("src"),
            "Focus field not properly set"
        );

        // Should still indicate success
        let success = structured.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        assert!(success, "Focus filter test returned success: false");

        let content_str = format!("{:?}", result.content);
        println!("Focus filtered content: {}", content_str);

        // Should mention the focus in output
        assert!(
            content_str.contains("focus") || content_str.contains("src") || content_str.contains("filter"),
            "Should indicate focus was applied. Got: {}",
            content_str
        );
    }

    /// Test that structured content is properly populated
    #[tokio::test]
    async fn test_fast_explore_structured_content_populated() {
        let (handler, _temp_dir) = create_test_workspace_with_data()
            .await
            .expect("Failed to create test workspace");

        let tool = FastExploreTool {
            mode: "overview".to_string(),
            depth: "medium".to_string(),
            focus: None,
        };

        let result = tool.call_tool(&handler).await.expect("Tool call failed");

        // The tool should return structured content
        let structured = result.structured_content.as_ref();
        assert!(
            structured.is_some(),
            "❌ BUG: structured_content is None! Tool returned no structured data."
        );

        let structured = structured.unwrap();

        // Check for required fields in structured content
        assert!(
            structured.contains_key("tool"),
            "Missing 'tool' field in structured content"
        );
        assert!(
            structured.contains_key("mode"),
            "Missing 'mode' field in structured content"
        );
        assert!(
            structured.contains_key("success"),
            "Missing 'success' field in structured content"
        );

        // Verify success is true
        let success = structured
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        assert!(success, "Tool returned success: false");
    }
}
