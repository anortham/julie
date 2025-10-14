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
        let source_path = PathBuf::from("tests/editing/sources").join(source_filename);
        let dest_path = temp_dir.path().join(source_filename);

        fs::copy(&source_path, &dest_path)?;
        Ok((temp_dir, dest_path))
    }

    // Test helper: Load control file
    fn load_control_file(control_filename: &str) -> Result<String> {
        let control_path = PathBuf::from("tests/editing/controls/line-edit").join(control_filename);
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
        let (_temp_dir, test_file) = setup_test_file("line_edit_base.py")?;

        // Load CONTROL (expected result)
        let expected_content = load_control_file("line_edit_insert_import.py")?;

        // Operation: Insert "import logging" at line 6 (after docstring)
        use crate::tools::edit_lines::EditLinesTool;
        use crate::handler::JulieServerHandler;

        let handler = JulieServerHandler::new().await?;

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

        let (_temp_dir, test_file) = setup_test_file("line_edit_base.py")?;
        let expected_content = load_control_file("line_edit_delete_comment.py")?;

        // Operation: Delete line 15 ("# Test data" comment)
        use crate::tools::edit_lines::EditLinesTool;
        use crate::handler::JulieServerHandler;

        let handler = JulieServerHandler::new().await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "delete".to_string(),
            start_line: 15,
            end_line: Some(15),  // Delete single line
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

        let (_temp_dir, test_file) = setup_test_file("line_edit_base.py")?;
        let expected_content = load_control_file("line_edit_replace_function_only.py")?;

        // Operation: Replace lines 7-12 (calculate_sum function) with calculate_average
        let replacement_content = r#"def calculate_average(numbers):
    """Calculate the average of a list of numbers."""
    total = 0
    for num in numbers:
        total += num
    return total / len(numbers) if numbers else 0"#;

        use crate::tools::edit_lines::EditLinesTool;
        use crate::handler::JulieServerHandler;

        let handler = JulieServerHandler::new().await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "replace".to_string(),
            start_line: 7,
            end_line: Some(12),  // Replace lines 7-12 inclusive
            content: Some(replacement_content.to_string()),
            dry_run: false,
        };

        edit_tool.call_tool(&handler).await?;
        verify_exact_match(&test_file, &expected_content)?;

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_lines_dry_run() -> Result<()> {
        // TDD RED: Verify dry_run doesn't modify file

        let (_temp_dir, test_file) = setup_test_file("line_edit_base.py")?;
        let original_content = fs::read_to_string(&test_file)?;

        use crate::tools::edit_lines::EditLinesTool;
        use crate::handler::JulieServerHandler;

        let handler = JulieServerHandler::new().await?;

        let edit_tool = EditLinesTool {
            file_path: test_file.to_string_lossy().to_string(),
            operation: "insert".to_string(),
            start_line: 6,
            end_line: None,
            content: Some("import logging".to_string()),
            dry_run: true,  // DRY RUN - should NOT modify file
        };

        edit_tool.call_tool(&handler).await?;

        // Verify: File content unchanged
        let after_content = fs::read_to_string(&test_file)?;
        assert_eq!(original_content, after_content, "dry_run should not modify file");

        Ok(())
    }
}
