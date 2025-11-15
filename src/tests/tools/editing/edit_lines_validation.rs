//! Tests for EditLinesTool input validation and error handling
//! TDD: RED → GREEN → REFACTOR
//! Tests comprehensive parameter validation without panics

#[cfg(test)]
mod edit_lines_validation_tests {
    use anyhow::Result;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn setup_test_file(content: &str, test_name: &str) -> Result<(TempDir, PathBuf)> {
        use crate::tests::helpers::workspace::create_unique_test_workspace;

        let temp_dir = create_unique_test_workspace(test_name)?;
        let test_file = temp_dir.path().join("test.py");
        fs::write(&test_file, content)?;
        Ok((temp_dir, test_file))
    }

    // ===== INSERT OPERATION VALIDATION =====

    #[tokio::test]
    #[serial_test::serial]
    async fn test_insert_missing_content_validation() -> Result<()> {
        //! TDD RED: insert without content should return graceful error
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) = setup_test_file("line 1\nline 2\n", "insert_missing_content")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "insert".to_string(),
            start_line: 1,
            end_line: None,
            content: None, // MISSING REQUIRED CONTENT
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        // Should fail gracefully with descriptive error
        assert!(
            result.is_err(),
            "Should fail when content is missing for insert"
        );
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("content") && error_msg.contains("required"),
            "Error message should be descriptive: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_insert_invalid_start_line_zero() -> Result<()> {
        //! TDD RED: start_line = 0 should be rejected (1-indexed)
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) = setup_test_file("line 1\nline 2\n", "insert_zero_line")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "insert".to_string(),
            start_line: 0, // INVALID: 0-indexed
            end_line: None,
            content: Some("new line".to_string()),
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(result.is_err(), "start_line=0 should be rejected");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("1-indexed") || error_msg.contains(">="),
            "Error should explain 1-indexing: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_insert_beyond_file_end() -> Result<()> {
        //! TDD RED: Inserting beyond file length should fail gracefully
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) = setup_test_file("line 1\nline 2\n", "insert_beyond_end")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "insert".to_string(),
            start_line: 999, // Beyond file
            end_line: None,
            content: Some("new line".to_string()),
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(result.is_err(), "Insert beyond file should fail");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("file only has") || error_msg.contains("Cannot insert"),
            "Error should explain file size: {}",
            error_msg
        );

        Ok(())
    }

    // ===== REPLACE OPERATION VALIDATION =====

    #[tokio::test]
    #[serial_test::serial]
    async fn test_replace_missing_end_line_validation() -> Result<()> {
        //! TDD RED: replace without end_line should return graceful error
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) =
            setup_test_file("line 1\nline 2\nline 3\n", "replace_missing_end")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "replace".to_string(),
            start_line: 1,
            end_line: None, // MISSING REQUIRED END_LINE
            content: Some("new content".to_string()),
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(
            result.is_err(),
            "Should fail when end_line is missing for replace"
        );
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("end_line") && error_msg.contains("required"),
            "Error message should mention end_line requirement: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_replace_missing_content_validation() -> Result<()> {
        //! TDD RED: replace without content should return graceful error
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) =
            setup_test_file("line 1\nline 2\nline 3\n", "replace_missing_content")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "replace".to_string(),
            start_line: 1,
            end_line: Some(2),
            content: None, // MISSING REQUIRED CONTENT
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(
            result.is_err(),
            "Should fail when content is missing for replace"
        );
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("content") && error_msg.contains("required"),
            "Error message should mention content requirement: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_replace_end_before_start() -> Result<()> {
        //! TDD RED: end_line < start_line should fail gracefully
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) =
            setup_test_file("line 1\nline 2\nline 3\n", "replace_backwards")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "replace".to_string(),
            start_line: 5,
            end_line: Some(2), // end < start
            content: Some("new".to_string()),
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(result.is_err(), "end_line < start_line should fail");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains(">=") || error_msg.contains("must be"),
            "Error should explain ordering: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_replace_start_beyond_file() -> Result<()> {
        //! TDD RED: start_line beyond file should fail gracefully
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) = setup_test_file("line 1\nline 2\n", "replace_beyond_start")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "replace".to_string(),
            start_line: 999,
            end_line: Some(1000),
            content: Some("new".to_string()),
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(result.is_err(), "start_line beyond file should fail");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Cannot replace") || error_msg.contains("file only has"),
            "Error should be descriptive: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_replace_end_beyond_file() -> Result<()> {
        //! TDD RED: end_line beyond file should fail gracefully
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) = setup_test_file("line 1\nline 2\n", "replace_beyond_end")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "replace".to_string(),
            start_line: 1,
            end_line: Some(999), // Beyond file
            content: Some("new".to_string()),
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(result.is_err(), "end_line beyond file should fail");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Cannot replace") || error_msg.contains("file only has"),
            "Error should be descriptive: {}",
            error_msg
        );

        Ok(())
    }

    // ===== DELETE OPERATION VALIDATION =====

    #[tokio::test]
    #[serial_test::serial]
    async fn test_delete_missing_end_line_validation() -> Result<()> {
        //! TDD RED: delete without end_line should return graceful error
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) =
            setup_test_file("line 1\nline 2\nline 3\n", "delete_missing_end")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "delete".to_string(),
            start_line: 1,
            end_line: None, // MISSING REQUIRED END_LINE
            content: None,
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(
            result.is_err(),
            "Should fail when end_line is missing for delete"
        );
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("end_line") && error_msg.contains("required"),
            "Error message should mention end_line requirement: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_delete_end_before_start() -> Result<()> {
        //! TDD RED: end_line < start_line should fail gracefully
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) =
            setup_test_file("line 1\nline 2\nline 3\n", "delete_backwards")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "delete".to_string(),
            start_line: 5,
            end_line: Some(2), // end < start
            content: None,
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(result.is_err(), "end_line < start_line should fail");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains(">=") || error_msg.contains("must be"),
            "Error should explain ordering: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_delete_start_beyond_file() -> Result<()> {
        //! TDD RED: start_line beyond file should fail gracefully
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) = setup_test_file("line 1\nline 2\n", "delete_beyond_start")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "delete".to_string(),
            start_line: 999,
            end_line: Some(1000),
            content: None,
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(result.is_err(), "start_line beyond file should fail");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Cannot delete") || error_msg.contains("file only has"),
            "Error should be descriptive: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_delete_end_beyond_file() -> Result<()> {
        //! TDD RED: end_line beyond file should fail gracefully
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) = setup_test_file("line 1\nline 2\n", "delete_beyond_end")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "delete".to_string(),
            start_line: 1,
            end_line: Some(999), // Beyond file
            content: None,
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(result.is_err(), "end_line beyond file should fail");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Cannot delete") || error_msg.contains("file only has"),
            "Error should be descriptive: {}",
            error_msg
        );

        Ok(())
    }

    // ===== OPERATION VALIDATION =====

    #[tokio::test]
    #[serial_test::serial]
    async fn test_invalid_operation_name() -> Result<()> {
        //! TDD RED: Invalid operation should fail gracefully
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) = setup_test_file("line 1\nline 2\n", "invalid_operation")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace_with_force(
                Some(temp_dir.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "invalidop".to_string(), // INVALID OPERATION
            start_line: 1,
            end_line: None,
            content: Some("new".to_string()),
            dry_run: true,
        };

        let result = edit_tool.call_tool(&handler).await;

        assert!(result.is_err(), "Invalid operation should fail");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Invalid operation")
                || error_msg.contains("insert")
                || error_msg.contains("replace")
                || error_msg.contains("delete"),
            "Error should list valid operations: {}",
            error_msg
        );

        Ok(())
    }
}
