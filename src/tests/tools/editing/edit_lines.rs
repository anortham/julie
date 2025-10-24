//! Tests for EditLinesTool - Surgical line-level editing
//! TDD: RED → GREEN → REFACTOR
//! Uses SOURCE/CONTROL golden master pattern

#[cfg(test)]
mod edit_lines_tests {
    use anyhow::Result;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // Test helper: Copy source file to temp directory
    fn setup_test_file(source_filename: &str) -> Result<(TempDir, PathBuf)> {
        let temp_dir = TempDir::new()?;
        let source_path = PathBuf::from("fixtures/editing/sources").join(source_filename);
        let dest_path = temp_dir.path().join(source_filename);

        fs::copy(&source_path, &dest_path)?;
        Ok((temp_dir, dest_path))
    }

    // Test helper: Load control file
    fn load_control_file(control_filename: &str) -> Result<String> {
        let control_path =
            PathBuf::from("fixtures/editing/controls/line-edit").join(control_filename);
        Ok(fs::read_to_string(control_path)?)
    }

    // Test helper: Verify exact match
    fn verify_exact_match(result_path: &PathBuf, expected_content: &str) -> Result<()> {
        let actual_content = fs::read_to_string(result_path)?;

        if actual_content != expected_content {
            // Show diff for debugging
            println!("❌ MISMATCH DETECTED");
            println!("Expected:\n{}", expected_content);
            println!("\nActual:\n{}", actual_content);
            anyhow::bail!("Content does not match control file");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_lines_insert_import() -> Result<()> {
        // TDD RED: This test WILL FAIL because EditLinesTool doesn't exist yet

        // Setup: Copy SOURCE to temp location
        let (temp_dir, test_file) = setup_test_file("line_edit_base.py")?;

        // Load CONTROL (expected result)
        let expected_content = load_control_file("line_edit_insert_import.py")?;

        // Operation: Insert "import logging" at line 6 (after docstring)
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "insert".to_string(),
            start_line: 6,
            end_line: None,
            content: Some("import logging".to_string()),
            dry_run: false,
        };

        edit_tool.call_tool(&handler).await?;

        // Verify: Result matches CONTROL exactly
        verify_exact_match(&test_file, &expected_content)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_lines_delete_comment() -> Result<()> {
        // TDD RED: This test WILL FAIL because EditLinesTool doesn't exist yet

        let (temp_dir, test_file) = setup_test_file("line_edit_base.py")?;
        let expected_content = load_control_file("line_edit_delete_comment.py")?;

        // Operation: Delete line 15 ("# Test data" comment)
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "delete".to_string(),
            start_line: 15,
            end_line: Some(15), // Delete single line
            content: None,
            dry_run: false,
        };

        edit_tool.call_tool(&handler).await?;
        verify_exact_match(&test_file, &expected_content)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_lines_replace_function() -> Result<()> {
        // TDD RED: This test WILL FAIL because EditLinesTool doesn't exist yet

        let (temp_dir, test_file) = setup_test_file("line_edit_base.py")?;
        let expected_content = load_control_file("line_edit_replace_function_only.py")?;

        // Operation: Replace lines 7-12 (calculate_sum function) with calculate_average
        let replacement_content = r#"def calculate_average(numbers):
    """Calculate the average of a list of numbers."""
    total = 0
    for num in numbers:
        total += num
    return total / len(numbers) if numbers else 0"#;

        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "replace".to_string(),
            start_line: 7,
            end_line: Some(12), // Replace lines 7-12 inclusive
            content: Some(replacement_content.to_string()),
            dry_run: false,
        };

        edit_tool.call_tool(&handler).await?;
        verify_exact_match(&test_file, &expected_content)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_lines_relative_path_uses_workspace_root() -> Result<()> {
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let (temp_dir, test_file) = setup_test_file("line_edit_base.py")?;
        let relative_path = test_file
            .file_name()
            .expect("filename")
            .to_string_lossy()
            .to_string();

        let expected_content = load_control_file("line_edit_insert_import.py")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        let edit_tool = EditLinesTool {
            file_path: relative_path,
            operation: "insert".to_string(),
            start_line: 6,
            end_line: None,
            content: Some("import logging".to_string()),
            dry_run: false,
        };

        edit_tool.call_tool(&handler).await?;
        verify_exact_match(&test_file, &expected_content)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_lines_preserves_crlf_line_endings() -> Result<()> {
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("windows_file.rs");
        let original_content = "fn main() {\r\n    println!(\"hello\");\r\n}\r\n";
        fs::write(&file_path, original_content)?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        let edit_tool = EditLinesTool {
            file_path: "windows_file.rs".to_string(),
            operation: "insert".to_string(),
            start_line: 2,
            end_line: None,
            content: Some("    // inserted comment".to_string()),
            dry_run: false,
        };

        edit_tool.call_tool(&handler).await?;

        let contents = String::from_utf8(fs::read(&file_path)?)?;
        assert!(
            contents.contains("\r\n"),
            "file should contain CRLF line endings: {}",
            contents
        );
        assert!(
            !contents.replace("\r\n", "").contains('\n'),
            "no bare LF newlines should remain: {}",
            contents
        );
        assert!(
            contents.contains("// inserted comment"),
            "inserted line should be present: {}",
            contents
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_lines_dry_run() -> Result<()> {
        // TDD RED: Verify dry_run doesn't modify file

        let (temp_dir, test_file) = setup_test_file("line_edit_base.py")?;
        let original_content = fs::read_to_string(&test_file)?;

        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "insert".to_string(),
            start_line: 6,
            end_line: None,
            content: Some("import logging".to_string()),
            dry_run: true, // DRY RUN - should NOT modify file
        };

        edit_tool.call_tool(&handler).await?;

        // Verify: File content unchanged
        let after_content = fs::read_to_string(&test_file)?;
        assert_eq!(
            original_content, after_content,
            "dry_run should not modify file"
        );

        Ok(())
    }

    // ===== SECURITY TESTS =====

    #[tokio::test]
    async fn test_path_traversal_prevention_absolute_path() -> Result<()> {
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let temp_dir = TempDir::new()?;
        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        // Try to access /etc/passwd using absolute path
        let edit_tool = EditLinesTool {
            file_path: "/etc/passwd".to_string(),
            operation: "insert".to_string(),
            start_line: 1,
            end_line: None,
            content: Some("malicious content".to_string()),
            dry_run: false,
        };

        let result = edit_tool.call_tool(&handler).await;

        // Should fail with security error
        assert!(
            result.is_err(),
            "Absolute path outside workspace should be blocked"
        );
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Security") || error_msg.contains("traversal"),
            "Error should mention security/traversal: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_path_traversal_prevention_relative_traversal() -> Result<()> {
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let temp_dir = TempDir::new()?;
        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        // Try to access ../../../../etc/passwd using relative path traversal
        let edit_tool = EditLinesTool {
            file_path: "../../../../etc/passwd".to_string(),
            operation: "insert".to_string(),
            start_line: 1,
            end_line: None,
            content: Some("malicious content".to_string()),
            dry_run: false,
        };

        let result = edit_tool.call_tool(&handler).await;

        // Should fail with security error or path not found (both secure outcomes)
        assert!(result.is_err(), "Relative path traversal should be blocked");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Security")
                || error_msg.contains("traversal")
                || error_msg.contains("does not exist"),
            "Error should indicate security block or non-existent path: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    #[cfg(unix)] // Symlink test only applicable on Unix systems
    async fn test_path_traversal_prevention_symlink_outside_workspace() -> Result<()> {
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;
        use std::os::unix::fs::symlink;

        let _temp_dir = TempDir::new()?;
        let workspace_dir = TempDir::new()?;

        // Create a symlink in workspace pointing outside
        let symlink_path = workspace_dir.path().join("evil_link");
        symlink("/etc/passwd", &symlink_path)?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(workspace_dir.path().to_string_lossy().to_string()))
            .await?;

        // Try to access through symlink
        let edit_tool = EditLinesTool {
            file_path: "evil_link".to_string(),
            operation: "insert".to_string(),
            start_line: 1,
            end_line: None,
            content: Some("malicious content".to_string()),
            dry_run: false,
        };

        let result = edit_tool.call_tool(&handler).await;

        // Should fail with security error
        assert!(
            result.is_err(),
            "Symlink outside workspace should be blocked"
        );
        let error_msg = format!("{}", result.unwrap_err());
        assert!(
            error_msg.contains("Security") || error_msg.contains("traversal"),
            "Error should mention security/traversal: {}",
            error_msg
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_secure_path_resolution_valid_paths() -> Result<()> {
        use crate::handler::JulieServerHandler;
        use crate::tools::edit_lines::EditLinesTool;

        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.py");
        fs::write(&test_file, "print('hello')")?;

        let handler = JulieServerHandler::new().await?;
        handler
            .initialize_workspace(Some(temp_dir.path().to_string_lossy().to_string()))
            .await?;

        // Valid absolute path should work
        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "insert".to_string(),
            start_line: 1,
            end_line: None,
            content: Some("# comment".to_string()),
            dry_run: false,
        };

        let result = edit_tool.call_tool(&handler).await;

        // Should succeed
        assert!(
            result.is_ok(),
            "Valid relative path should work: {:?}",
            result
        );

        // Verify the file was actually modified
        let content = fs::read_to_string(&test_file)?;
        assert!(
            content.contains("# comment"),
            "File should contain inserted comment"
        );

        Ok(())
    }
}
