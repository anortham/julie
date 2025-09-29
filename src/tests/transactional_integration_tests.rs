use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::editing::{FastEditTool, LineEditTool};

/// Integration tests to verify FastEditTool and LineEditTool work correctly
/// with transactional safety integration
///
/// These tests ensure backward compatibility while adding atomic file operation safety
#[cfg(test)]
mod transactional_integration_tests {
    use super::*;

    /// Test helper for transactional integration testing
    pub struct TransactionalIntegrationFixture {
        temp_dir: TempDir,
    }

    impl TransactionalIntegrationFixture {
        pub fn new() -> Result<Self> {
            Ok(Self {
                temp_dir: tempfile::tempdir()?,
            })
        }

        pub fn create_test_file(&self, name: &str, content: &str) -> Result<String> {
            let file_path = self.temp_dir.path().join(name);
            fs::write(&file_path, content)?;
            Ok(file_path.to_string_lossy().to_string())
        }

        pub fn read_test_file(&self, path: &str) -> Result<String> {
            Ok(fs::read_to_string(path)?)
        }

        /// Verify no .tmp files were left behind (transaction cleanup verification)
        pub fn verify_no_temp_files(&self) -> Result<()> {
            for entry in fs::read_dir(self.temp_dir.path())? {
                let entry = entry?;
                let file_name = entry.file_name();
                let name = file_name.to_string_lossy();

                assert!(
                    !name.contains(".tmp."),
                    "Found temp file that should have been cleaned up: {}",
                    name
                );
            }
            Ok(())
        }
    }

    // ==========================================
    // Integration Test Cases (TDD Style)
    // ==========================================

    #[tokio::test]
    async fn test_fast_edit_tool_with_transaction_success() -> Result<()> {
        println!("🧪 Testing FastEditTool integration with transactional safety...");

        let fixture = TransactionalIntegrationFixture::new()?;
        let file_path = fixture.create_test_file("test.js", r#"
function getUserData() {
    return userData;
}
"#)?;

        // Create FastEditTool with transactional integration
        let edit_tool = FastEditTool {
            file_path: file_path.clone(),
            find_text: "getUserData".to_string(),
            replace_text: "fetchUserData".to_string(),
            mode: None,
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,
            dry_run: false,
        };

        // This should now use EditingTransaction internally
        let handler = JulieServerHandler::new().await?;
        let result = edit_tool.call_tool(&handler).await?;

        // Verify successful result
        assert!(result.content.len() > 0);

        // Verify file was modified correctly
        let final_content = fixture.read_test_file(&file_path)?;
        assert!(final_content.contains("fetchUserData"));
        assert!(!final_content.contains("getUserData"));

        // Verify no temp files left behind
        fixture.verify_no_temp_files()?;

        println!("✅ FastEditTool transactional integration test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_line_edit_tool_with_transaction_success() -> Result<()> {
        println!("🧪 Testing LineEditTool integration with transactional safety...");

        let fixture = TransactionalIntegrationFixture::new()?;
        let file_path = fixture.create_test_file("test.py", r#"def process_data():
    # TODO: implement this
    pass
"#)?;

        // Create LineEditTool with transactional integration
        let edit_tool = LineEditTool {
            file_path: file_path.clone(),
            operation: "replace".to_string(),
            start_line: Some(2),
            end_line: Some(2),
            line_number: None,
            content: Some("    return process_user_data()".to_string()),
            preserve_indentation: true,
            dry_run: false,
        };

        // This should now use EditingTransaction internally
        let handler = JulieServerHandler::new().await?;
        let result = edit_tool.call_tool(&handler).await?;

        // Verify successful result
        assert!(result.content.len() > 0);

        // Verify file was modified correctly
        let final_content = fixture.read_test_file(&file_path)?;
        assert!(final_content.contains("return process_user_data()"));
        assert!(!final_content.contains("TODO: implement this"));

        // Verify no temp files left behind
        fixture.verify_no_temp_files()?;

        println!("✅ LineEditTool transactional integration test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_rollback_on_validation_failure() -> Result<()> {
        println!("🧪 Testing transaction rollback on validation failure...");

        let fixture = TransactionalIntegrationFixture::new()?;
        let file_path = fixture.create_test_file("test.js", r#"
function validFunction() {
    return { data: "test" };
}
"#)?;

        let original_content = fixture.read_test_file(&file_path)?;

        // Create edit that would break syntax (unmatched braces)
        let edit_tool = FastEditTool {
            file_path: file_path.clone(),
            find_text: "{".to_string(),
            replace_text: "{{".to_string(), // This will create unmatched braces
            mode: None,
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,
            dry_run: false,
        };

        let handler = JulieServerHandler::new().await?;
        let result = edit_tool.call_tool(&handler).await?;

        // Should report validation failure
        assert!(result.content.len() > 0);

        // Verify original file was preserved due to transaction rollback
        let final_content = fixture.read_test_file(&file_path)?;
        assert_eq!(final_content, original_content);

        // Verify no temp files left behind
        fixture.verify_no_temp_files()?;

        println!("✅ Transaction rollback integration test passed");
        Ok(())
    }

    #[tokio::test]
    async fn test_dry_run_mode_with_transactions() -> Result<()> {
        println!("🧪 Testing dry-run mode works with transactional integration...");

        let fixture = TransactionalIntegrationFixture::new()?;
        let file_path = fixture.create_test_file("test.rs", r#"
fn calculate_result() -> i32 {
    42
}
"#)?;

        let original_content = fixture.read_test_file(&file_path)?;

        // Test dry-run mode
        let edit_tool = FastEditTool {
            file_path: file_path.clone(),
            find_text: "calculate_result".to_string(),
            replace_text: "compute_result".to_string(),
            mode: None,
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,
            dry_run: true,
        };

        let handler = JulieServerHandler::new().await?;
        let result = edit_tool.call_tool(&handler).await?;

        // Should show preview
        assert!(result.content.len() > 0);

        // Verify file was NOT modified (dry-run mode)
        let final_content = fixture.read_test_file(&file_path)?;
        assert_eq!(final_content, original_content);

        // Verify no temp files left behind
        fixture.verify_no_temp_files()?;

        println!("✅ Dry-run mode integration test passed");
        Ok(())
    }
}