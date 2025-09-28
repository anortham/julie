//! CRITICAL SAFETY TESTS FOR EDITING TOOLS
//!
//! These tests cover scenarios that could lead to data corruption or security issues.
//! All tests must pass before any editing tool can be considered safe for production.

use anyhow::Result;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

use crate::handler::JulieServerHandler;
use crate::tools::editing::{FastEditTool, LineEditTool};
use rust_mcp_sdk::schema::CallToolResult;

/// Extract text from CallToolResult safely
fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str().map(|s| s.to_string()))
            })
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Test fixture for safety testing
struct SafetyTestFixture {
    temp_dir: TempDir,
}

impl SafetyTestFixture {
    fn new() -> Result<Self> {
        Ok(Self {
            temp_dir: TempDir::new()?,
        })
    }

    fn create_test_file(&self, name: &str, content: &str) -> Result<String> {
        let file_path = self.temp_dir.path().join(name);
        fs::write(&file_path, content)?;
        Ok(file_path.to_string_lossy().to_string())
    }

    fn make_readonly(&self, file_path: &str) -> Result<()> {
        let path = std::path::Path::new(file_path);
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o444); // Read-only
        fs::set_permissions(path, perms)?;
        Ok(())
    }

    fn restore_permissions(&self, file_path: &str) -> Result<()> {
        let path = std::path::Path::new(file_path);
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o644); // Read-write
        fs::set_permissions(path, perms)?;
        Ok(())
    }
}

//****************************//
//  CONCURRENCY SAFETY TESTS //
//****************************//

#[cfg(test)]
mod concurrency_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_fast_edit_same_file() {
        let fixture = SafetyTestFixture::new().unwrap();
        let file_path = fixture
            .create_test_file("concurrent.txt", "original content\nline 2\nline 3")
            .unwrap();

        // Launch two concurrent edits on the same file
        let file_path_1 = file_path.clone();
        let file_path_2 = file_path.clone();

        let task1 = tokio::spawn(async move {
            let handler_1 = JulieServerHandler::new().await.unwrap();
            let tool = FastEditTool {
                file_path: file_path_1,
                find_text: "original".to_string(),
                replace_text: "modified1".to_string(),
                mode: None,
                language: None,
                file_pattern: None,
                limit: None,
                validate: true,

                dry_run: false,
            };
            tool.call_tool(&handler_1).await
        });

        let task2 = tokio::spawn(async move {
            sleep(Duration::from_millis(10)).await; // Slight delay
            let handler_2 = JulieServerHandler::new().await.unwrap();
            let tool = FastEditTool {
                file_path: file_path_2,
                find_text: "line 2".to_string(),
                replace_text: "modified2".to_string(),
                mode: None,
                language: None,
                file_pattern: None,
                limit: None,
                validate: true,

                dry_run: false,
            };
            tool.call_tool(&handler_2).await
        });

        let (result1, result2) = tokio::join!(task1, task2);

        // Both operations should complete without corruption
        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // File should be in a valid state (either one change or both)
        let final_content = fs::read_to_string(&file_path).unwrap();
        assert!(!final_content.is_empty());
        assert!(final_content.contains("line 3")); // Unchanged line should remain

        // NOTE: No backup file assertions - backup functionality was intentionally removed
    }

    #[tokio::test]
    async fn test_concurrent_line_edit_same_file() {
        let fixture = SafetyTestFixture::new().unwrap();
        let file_path = fixture
            .create_test_file("concurrent_lines.txt", "line1\nline2\nline3\nline4\nline5")
            .unwrap();

        // Launch concurrent line edits
        let file_path_1 = file_path.clone();
        let file_path_2 = file_path.clone();

        let task1 = tokio::spawn(async move {
            let handler_1 = JulieServerHandler::new().await.unwrap();
            let tool = LineEditTool {
                file_path: file_path_1,
                operation: "replace".to_string(),
                start_line: Some(1),
                end_line: Some(1),
                line_number: None,
                content: Some("REPLACED1".to_string()),
                preserve_indentation: true,

                dry_run: false,
            };
            tool.call_tool(&handler_1).await
        });

        let task2 = tokio::spawn(async move {
            sleep(Duration::from_millis(5)).await;
            let handler_2 = JulieServerHandler::new().await.unwrap();
            let tool = LineEditTool {
                file_path: file_path_2,
                operation: "replace".to_string(),
                start_line: Some(5),
                end_line: Some(5),
                line_number: None,
                content: Some("REPLACED5".to_string()),
                preserve_indentation: true,

                dry_run: false,
            };
            tool.call_tool(&handler_2).await
        });

        let (result1, result2) = tokio::join!(task1, task2);

        // Both should succeed or fail gracefully
        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // File should not be corrupted
        let final_content = fs::read_to_string(&file_path).unwrap();
        let lines: Vec<&str> = final_content.lines().collect();
        assert_eq!(lines.len(), 5); // Still 5 lines
    }
}

//***************************//
//  PERMISSION SAFETY TESTS //
//***************************//

#[cfg(test)]
mod permission_tests {
    use super::*;

    #[tokio::test]
    async fn test_readonly_file_handling() {
        let fixture = SafetyTestFixture::new().unwrap();
        let file_path = fixture
            .create_test_file("readonly.txt", "readonly content")
            .unwrap();

        // Make file read-only
        fixture.make_readonly(&file_path).unwrap();

        let tool = FastEditTool {
            file_path: file_path.clone(),
            find_text: "readonly".to_string(),
            replace_text: "modified".to_string(),
            mode: None,
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,

            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should gracefully handle permission error
        let response = extract_text_from_result(&result);
        assert!(response.contains("Failed to write") || response.contains("permission"));

        // Original file should be unchanged
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "readonly content");

        // Cleanup
        fixture.restore_permissions(&file_path).unwrap();
    }

    #[tokio::test]
    async fn test_directory_readonly_handling() {
        let fixture = SafetyTestFixture::new().unwrap();
        let file_path = fixture.create_test_file("file.txt", "content").unwrap();

        // Make parent directory read-only
        let parent_dir = std::path::Path::new(&file_path).parent().unwrap();
        let mut perms = fs::metadata(parent_dir).unwrap().permissions();
        perms.set_mode(0o555); // Read-only directory
        fs::set_permissions(parent_dir, perms).unwrap();

        let tool = FastEditTool {
            file_path: file_path.clone(),
            find_text: "content".to_string(),
            replace_text: "new content".to_string(),
            mode: None,
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,

            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await;

        // Should handle backup creation failure gracefully
        assert!(result.is_ok());

        // Restore directory permissions for cleanup
        let mut perms = fs::metadata(parent_dir).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(parent_dir, perms).unwrap();
    }
}

//*************************//
//  ENCODING SAFETY TESTS //
//*************************//

#[cfg(test)]
mod encoding_tests {
    use super::*;

    #[tokio::test]
    async fn test_utf8_handling() {
        let fixture = SafetyTestFixture::new().unwrap();
        let utf8_content = "Hello 世界 🌍 café naïve résumé";
        let file_path = fixture.create_test_file("utf8.txt", utf8_content).unwrap();

        let tool = FastEditTool {
            file_path: file_path.clone(),
            find_text: "世界".to_string(),
            replace_text: "world".to_string(),
            mode: None,
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,

            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should succeed without corruption
        let response = extract_text_from_result(&result);
        assert!(response.contains("successful") || response.contains("replaced"));

        // Verify UTF-8 is preserved
        let final_content = fs::read_to_string(&file_path).unwrap();
        assert!(final_content.contains("🌍"));
        assert!(final_content.contains("café"));
        assert!(final_content.contains("world"));
    }

    #[tokio::test]
    async fn test_binary_file_rejection() {
        let fixture = SafetyTestFixture::new().unwrap();

        // Create a binary file with null bytes
        let binary_data = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD];
        let file_path = fixture.temp_dir.path().join("binary.dat");
        fs::write(&file_path, binary_data).unwrap();

        let tool = FastEditTool {
            file_path: file_path.to_string_lossy().to_string(),
            find_text: "text".to_string(),
            replace_text: "replacement".to_string(),
            mode: None,
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,

            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should handle gracefully (either skip or error)
        let response = extract_text_from_result(&result);
        // Don't assert specific error - just ensure it doesn't crash
        assert!(!response.is_empty());
    }
}

//****************************//
//  PATH TRAVERSAL SAFETY    //
//****************************//

#[cfg(test)]
mod security_tests {
    use super::*;

    #[tokio::test]
    async fn test_path_traversal_prevention() {
        let _fixture = SafetyTestFixture::new().unwrap();

        // Try to edit a file outside the temp directory
        let malicious_path = "../../../etc/passwd";

        let tool = FastEditTool {
            file_path: malicious_path.to_string(),
            find_text: "root".to_string(),
            replace_text: "hacked".to_string(),
            mode: None,
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,

            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should fail to find file or refuse to edit
        let response = extract_text_from_result(&result);
        assert!(response.contains("not found") || response.contains("Failed"));

        // Ensure /etc/passwd wasn't touched (if it exists)
        if std::path::Path::new("/etc/passwd").exists() {
            let passwd_content = fs::read_to_string("/etc/passwd").unwrap();
            assert!(!passwd_content.contains("hacked"));
        }
    }

    #[tokio::test]
    async fn test_symlink_handling() {
        let fixture = SafetyTestFixture::new().unwrap();
        let target_file = fixture
            .create_test_file("target.txt", "target content")
            .unwrap();

        let symlink_path = fixture.temp_dir.path().join("symlink.txt");

        // Create symlink (Unix only)
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target_file, &symlink_path).unwrap();

            let tool = FastEditTool {
                file_path: symlink_path.to_string_lossy().to_string(),
                find_text: "target".to_string(),
                replace_text: "modified".to_string(),
                mode: None,
                language: None,
                file_pattern: None,
                limit: None,
                validate: true,

                dry_run: false,
            };

            let handler = JulieServerHandler::new().await.unwrap();
            let result = tool.call_tool(&handler).await.unwrap();

            // Should handle symlinks correctly (follow them)
            let _response = extract_text_from_result(&result);

            // Verify target file was modified, not the symlink
            let target_content = fs::read_to_string(&target_file).unwrap();
            assert!(target_content.contains("modified"));
        }
    }
}

//*****************************//
//  LARGE FILE SAFETY TESTS   //
//*****************************//

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[tokio::test]
    async fn test_large_file_memory_safety() {
        let fixture = SafetyTestFixture::new().unwrap();

        // Create a moderately large file (100KB - smaller for test speed)
        let large_content = "line of text\n".repeat(8_000); // ~100KB
        let file_path = fixture
            .create_test_file("large.txt", &large_content)
            .unwrap();

        let tool = FastEditTool {
            file_path: file_path.clone(),
            find_text: "line of text".to_string(),
            replace_text: "modified line".to_string(),
            mode: None,
            language: None,
            file_pattern: None,
            limit: None,
            validate: true,

            dry_run: true, // Dry run to avoid huge file modification
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let start_time = std::time::Instant::now();
        let result = tool.call_tool(&handler).await.unwrap();
        let duration = start_time.elapsed();

        // Should complete in reasonable time (< 5 seconds)
        assert!(duration.as_secs() < 5);

        // Should not crash or hang
        let response = extract_text_from_result(&result);
        assert!(response.contains("Dry run") || response.contains("preview"));
    }

    #[tokio::test]
    async fn test_extremely_long_lines() {
        let fixture = SafetyTestFixture::new().unwrap();

        // Create file with extremely long line (10KB line)
        let long_line = "a".repeat(10_000);
        let content = format!("short line\n{}\nshort line", long_line);
        let file_path = fixture
            .create_test_file("long_lines.txt", &content)
            .unwrap();

        let tool = LineEditTool {
            file_path: file_path.clone(),
            operation: "read".to_string(),
            start_line: Some(2),
            end_line: Some(2),
            line_number: None,
            content: None,
            preserve_indentation: true,

            dry_run: false,
        };

        let handler = JulieServerHandler::new().await.unwrap();
        let result = tool.call_tool(&handler).await.unwrap();

        // Should handle long lines without crashing
        let response = extract_text_from_result(&result);
        assert!(!response.is_empty());
    }
}
